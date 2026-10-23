use std::future::{Future, pending};
use std::panic;
use std::time::Duration;

use service_runtime::{Console, Shutdown, TaskCounts, TaskState};
use tokio::sync::oneshot;
use tokio::time::{advance, sleep, timeout};

const DEADLINE: Duration = Duration::from_secs(10);

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

async fn within<T>(what: &str, work: impl Future<Output = T>) -> T {
    timeout(DEADLINE, work)
        .await
        .unwrap_or_else(|_| panic!("{what}: не уложилось в {DEADLINE:?}, похоже на зависание"))
}

fn spin(busy: Duration) {
    let started = std::time::Instant::now();
    while started.elapsed() < busy {
        std::hint::spin_loop();
    }
}

fn explode() {
    panic::resume_unwind(Box::new("задача упала"));
}

async fn settle() {
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn spawned_task_is_registered_until_it_ends() {
    let console = Console::new();
    let (go, wait) = oneshot::channel::<u32>();
    let handle = console.spawn("answer", async move { wait.await.unwrap_or_default() + 1 });
    settle().await;
    let tasks = console.tasks();
    assert_eq!(tasks.len(), 1, "живая задача видна в консоли");
    let task = &tasks[0];
    assert_eq!(
        (task.id, task.name, task.state, task.polls),
        (1, "answer", TaskState::Idle, 1),
        "номера с единицы, имя из spawn, задача один раз опрошена и ждёт"
    );
    let _ = go.send(41);
    let answer = within("задача", handle).await.expect("задача не падает");
    assert_eq!(answer, 42, "результат доходит до JoinHandle без изменений");
    assert_eq!(
        console.counts(),
        TaskCounts {
            alive: 0,
            spawned: 1,
            finished: 1,
            cancelled: 0,
            panicked: 0,
        },
        "завершённая задача уходит из живых и считается завершённой"
    );
}

#[tokio::test(start_paused = true)]
async fn polls_and_waiting_time_are_tracked() {
    let console = Console::new();
    let (go, wait) = oneshot::channel::<()>();
    let handle = console.spawn("yielder", async move {
        for _ in 0..3 {
            tokio::task::yield_now().await;
        }
        let _ = wait.await;
    });
    settle().await;
    advance(ms(600)).await;
    let task = console.tasks()[0].clone();
    assert_eq!(
        (task.state, task.polls, task.since),
        (TaskState::Idle, 4, ms(600)),
        "три yield_now и ожидание это четыре poll, а since считает время с конца последнего poll"
    );
    assert_eq!(
        task.to_string(),
        "#1 yielder: ждёт 600 мс, poll 4",
        "строка задачи в дампе"
    );
    let _ = go.send(());
    within("задача", handle).await.expect("задача не падает");
}

#[tokio::test(start_paused = true)]
async fn aborted_and_panicked_tasks_are_counted() {
    let console = Console::new();
    let stuck = console.spawn("stuck", pending::<()>());
    let broken = console.spawn("broken", async { explode() });
    settle().await;
    stuck.abort();
    let _ = within("брошенная задача", stuck).await;
    let failed = within("упавшая задача", broken).await;
    assert!(failed.is_err(), "паника доходит до JoinHandle как ошибка");
    let counts = console.counts();
    assert_eq!(
        (
            counts.alive,
            counts.cancelled,
            counts.panicked,
            counts.finished
        ),
        (0, 1, 1, 0),
        "abort это брошенная задача, паника это паника, живых не осталось: {counts:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn traced_future_counts_wherever_it_runs() {
    let console = Console::new();
    let mut set = tokio::task::JoinSet::new();
    for n in 0..3 {
        set.spawn(console.traced("import", async move { n }));
    }
    let lazy = console.traced("lazy", async {});
    assert_eq!(
        console.alive(),
        4,
        "traced регистрирует future сразу, даже если его запустит JoinSet или он ещё не опрошен"
    );
    while let Some(done) = within("JoinSet", set.join_next()).await {
        done.expect("задача не падает");
    }
    drop(lazy);
    let counts = console.counts();
    assert_eq!(
        (counts.alive, counts.finished, counts.cancelled),
        (0, 3, 1),
        "три задачи JoinSet завершились, а брошенный без единого poll future считается брошенным"
    );
}

#[tokio::test(start_paused = true)]
async fn dump_lists_live_tasks_in_order() {
    let console = Console::new();
    console.spawn("stuck", pending::<()>());
    within("быстрая задача", console.spawn("done", async {}))
        .await
        .expect("задача не падает");
    console.spawn("sleeper", sleep(Duration::from_secs(3600)));
    settle().await;
    advance(ms(1500)).await;
    assert_eq!(
        console.dump(),
        "задач живых 2, запущено 3, завершено 1, брошено 0, паник 0\n\
         #1 stuck: ждёт 1500 мс, poll 1\n\
         #3 sleeper: ждёт 1500 мс, poll 1",
        "дамп: строка счётчиков и по строке на живую задачу в порядке номеров"
    );
}

#[tokio::test]
async fn watchdog_reports_long_polls() {
    let console = Console::with_threshold(ms(20));
    let hog = console.spawn("hog", async {
        spin(ms(60));
    });
    console
        .spawn("fine", async {})
        .await
        .expect("задача не падает");
    within("задача", hog).await.expect("задача не падает");
    let long = console.long_polls();
    let hog = long
        .iter()
        .find(|poll| poll.name == "hog")
        .unwrap_or_else(|| panic!("сторож называет задачу с долгим poll по имени: {long:?}"));
    assert!(
        hog.took >= ms(60),
        "сторож меряет, сколько длился poll: {:?}",
        hog.took
    );
    assert!(
        console.dump().contains("долгий poll: #1 hog "),
        "долгие poll попадают в дамп:\n{}",
        console.dump()
    );
}

#[tokio::test(start_paused = true)]
async fn stages_spawn_through_the_console() {
    let mut shutdown = Shutdown::with_console(Console::new());
    let stage = shutdown.stage("workers");
    let token = stage.token.clone();
    stage.spawn("worker", async move { token.cancelled().await });
    settle().await;
    let names: Vec<&str> = shutdown
        .console()
        .tasks()
        .iter()
        .map(|task| task.name)
        .collect();
    assert_eq!(
        names,
        ["worker"],
        "Stage::spawn регистрирует задачу в консоли Shutdown"
    );
    assert_eq!(stage.tasks.len(), 1, "и учитывает её в трекере стадии");
    within("остановка", shutdown.close(ms(100))).await;
    assert_eq!(
        shutdown.console().alive(),
        0,
        "после остановки живых задач не осталось"
    );
}
