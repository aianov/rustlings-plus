use std::future::pending;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use console::{Console, State};
use tokio::time::{advance, sleep};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

async fn settle() {
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn a_task_is_registered_and_returns_its_result() {
    let console = Console::new();
    let handle = console.spawn("answer", async { 6 * 7 });
    assert_eq!(handle.await.unwrap(), 42, "результат задачи доходит до владельца ручки без изменений");
    let tasks = console.tasks();
    assert_eq!(tasks.len(), 1, "в реестре одна запись");
    let task = &tasks[0];
    assert_eq!((task.id, task.name.as_str(), task.state, task.polls), (1, "answer", State::Finished, 1), "номер 1, имя, завершена, один опрос");
    assert_eq!(task.waiting_for, None, "завершённая задача ничего не ждёт");
}

#[tokio::test(start_paused = true)]
async fn ids_grow_and_names_are_kept() {
    let console = Console::new();
    let a = console.spawn("alpha", async {});
    let b = console.spawn("beta", async {});
    let c = console.spawn("gamma", async {});
    for handle in [a, b, c] {
        handle.await.unwrap();
    }
    let seen: Vec<(u64, String)> = console.tasks().into_iter().map(|t| (t.id, t.name)).collect();
    assert_eq!(seen, vec![(1, "alpha".to_string()), (2, "beta".to_string()), (3, "gamma".to_string())], "номера идут подряд с единицы в порядке создания");
}

#[tokio::test(start_paused = true)]
async fn polls_are_counted() {
    let console = Console::new();
    let handle = console.spawn("yielder", async {
        for _ in 0..3 {
            tokio::task::yield_now().await;
        }
    });
    handle.await.unwrap();
    assert_eq!(console.tasks()[0].polls, 4, "три отдачи хода означают четыре опроса");
}

#[tokio::test(start_paused = true)]
async fn waiting_time_follows_the_clock_and_a_wake_ends_it() {
    let console = Console::new();
    let handle = console.spawn("nap", async { sleep(ms(1000)).await });
    settle().await;
    let task = &console.tasks()[0];
    assert_eq!((task.state, task.polls, task.waiting_for), (State::Waiting, 1, Some(Duration::ZERO)), "задача заснула сразу");
    advance(ms(600)).await;
    settle().await;
    assert_eq!(console.tasks()[0].waiting_for, Some(ms(600)), "ждёт уже 600 мс");
    advance(ms(400)).await;
    handle.await.unwrap();
    let task = &console.tasks()[0];
    assert_eq!((task.state, task.polls, task.waiting_for), (State::Finished, 2, None), "проснулась, дошла до конца за два опроса");
}

#[tokio::test(start_paused = true)]
async fn dump_has_an_exact_format() {
    let console = Console::new();
    console.spawn("stuck", pending::<()>());
    console.spawn("done", async {}).await.unwrap();
    settle().await;
    advance(ms(1500)).await;
    settle().await;
    assert_eq!(
        console.dump(),
        "tasks alive=1 total=2\n1 stuck Waiting polls=1 waiting=1500ms\n2 done Finished polls=1 waiting=-",
        "первая строка со счётчиками, дальше по строке на задачу"
    );
}

#[tokio::test(start_paused = true)]
async fn stuck_lists_long_waiters_by_name() {
    let console = Console::new();
    console.spawn("old", pending::<()>());
    settle().await;
    advance(ms(2500)).await;
    console.spawn("young", pending::<()>());
    settle().await;
    advance(ms(500)).await;
    settle().await;
    assert_eq!(console.stuck(ms(1000)), vec!["old".to_string()], "дольше секунды ждёт только old: young ждёт 500 мс");
    assert_eq!(console.stuck(ms(10_000)), Vec::<String>::new(), "порог выше всех ожиданий: застрявших нет");
    assert_eq!(console.stuck(ms(500)), vec!["old".to_string()], "порог строгий: ожидание ровно в 500 мс не считается долгим");
}

#[tokio::test(start_paused = true)]
async fn a_panic_is_recorded_and_still_reaches_the_owner() {
    let console = Console::new();
    let handle = console.spawn("boom", async { panic!("взорвалась") });
    let error = handle.await.unwrap_err();
    assert!(error.is_panic(), "владелец ручки должен получить именно панику, а не отмену");
    let task = &console.tasks()[0];
    assert_eq!(task.state, State::Panicked, "в реестре задача помечена паникой");
    assert_eq!(console.alive(), 0, "упавшая задача не жива");
}

#[tokio::test(start_paused = true)]
async fn an_abort_is_recorded_as_cancelled() {
    let console = Console::new();
    let handle = console.spawn("long", async { sleep(Duration::from_secs(3600)).await });
    settle().await;
    handle.abort();
    let _ = handle.await;
    settle().await;
    assert_eq!(console.tasks()[0].state, State::Cancelled, "прерванная задача помечена отменённой");
    assert_eq!(console.alive(), 0, "и живой не считается");
}

#[tokio::test(start_paused = true)]
async fn alive_and_prune_agree() {
    let console = Console::new();
    for i in 0..5 {
        console.spawn(&format!("quick{i}"), async {}).await.unwrap();
    }
    console.spawn("forever1", pending::<()>());
    console.spawn("forever2", pending::<()>());
    settle().await;
    assert_eq!(console.alive(), 2, "две задачи не кончаются");
    assert_eq!(console.prune(), 5, "prune убирает пять завершённых записей");
    let names: Vec<String> = console.tasks().into_iter().map(|t| t.name).collect();
    assert_eq!(names, vec!["forever1".to_string(), "forever2".to_string()], "остаются только живые, порядок сохранён");
}

#[tokio::test]
async fn a_long_poll_triggers_the_hook_and_the_hook_may_look_at_the_console() {
    let console = Console::new();
    let seen = Arc::new(Mutex::new(Vec::new()));
    {
        let (console, seen) = (console.clone(), seen.clone());
        console.clone().on_long_poll(ms(10), move |name, took| {
            let snapshot = console.tasks().len();
            seen.lock().unwrap().push((name.to_string(), took, snapshot));
        });
    }
    console
        .spawn("blocker", async {
            std::thread::sleep(ms(40));
        })
        .await
        .unwrap();
    console.spawn("polite", async { tokio::task::yield_now().await }).await.unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1, "сторож сработал один раз: только блокирующий poll длиннее порога");
    assert_eq!(seen[0].0, "blocker");
    assert!(seen[0].1 >= ms(40), "длительность {:?} должна быть не меньше фактической", seen[0].1);
    assert!(seen[0].2 >= 1, "обратный вызов видит реестр: замок не должен быть занят");
    assert!(console.tasks()[0].longest_poll >= ms(40), "самый долгий опрос записан в реестр");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn many_tasks_from_many_threads() {
    let console = Console::new();
    let mut handles = Vec::new();
    for i in 0..200u64 {
        let inner = console.clone();
        handles.push(console.spawn(&format!("t{i}"), async move {
            tokio::task::yield_now().await;
            if i % 50 == 0 {
                let child = inner.spawn("child", async { 1u64 });
                return child.await.unwrap() + i;
            }
            i
        }));
    }
    let mut sum = 0;
    for handle in handles {
        sum += handle.await.unwrap();
    }
    assert_eq!(sum, (0..200u64).sum::<u64>() + 4, "результаты не потерялись: четыре задачи прибавили по единице от детей");
    assert_eq!(console.alive(), 0, "все задачи закончились");
    assert_eq!(console.tasks().len(), 204, "200 основных и 4 дочерних записи");
    assert!(console.tasks().iter().all(|t| t.state == State::Finished && t.polls >= 1), "все записи завершены и опрошены");
}
