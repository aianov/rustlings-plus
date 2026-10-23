use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::{FutureExt, StreamExt};
use jobq::{Closed, Config, JobSpec, Report, Scheduler, Status};
use tokio::runtime::Handle;
use tokio::time::{Instant, sleep, timeout};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn spec<F, Fut>(name: &str, priority: u8, timeout_ms: u64, run: F) -> JobSpec
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<String, String>> + Send + 'static,
{
    JobSpec { name: name.to_string(), priority, timeout: ms(timeout_ms), run: Box::new(move || run().boxed()) }
}

fn sleeper(name: &str, priority: u8, work_ms: u64) -> JobSpec {
    let text = name.to_string();
    spec(name, priority, 60_000, move || {
        let text = text.clone();
        async move {
            sleep(ms(work_ms)).await;
            Ok(text)
        }
    })
}

fn config(workers: usize) -> Config {
    Config { workers, retries: 0, backoff: ms(100) }
}

async fn settle() {
    for _ in 0..200 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn a_job_runs_and_its_result_is_waited_for() {
    let (jobs, _runner) = Scheduler::start(config(2));
    let first = jobs.submit(sleeper("первое", 1, 10)).await.unwrap();
    let second = jobs.submit(sleeper("второе", 1, 20)).await.unwrap();
    assert_eq!((first, second), (1, 2), "номера заданий идут с единицы подряд");
    assert_eq!(jobs.wait(first).await.unwrap(), Some(Status::Done("первое".to_string())), "wait отдаёт итог задания");
    assert_eq!(jobs.status(second).await.unwrap(), Some(Status::Running), "второе ещё идёт");
    assert_eq!(jobs.wait(second).await.unwrap(), Some(Status::Done("второе".to_string())));
    assert_eq!(jobs.wait(first).await.unwrap(), Some(Status::Done("первое".to_string())), "wait по уже завершённому отвечает сразу");
    assert_eq!(jobs.status(99).await.unwrap(), None, "неизвестный номер: None");
    assert_eq!(jobs.wait(99).await.unwrap(), None, "wait по неизвестному номеру тоже None, а не вечное ожидание");
}

#[tokio::test(start_paused = true)]
async fn at_most_workers_jobs_run_at_once() {
    let (jobs, _runner) = Scheduler::start(config(2));
    let running = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let started = Instant::now();
    let mut ids = Vec::new();
    for i in 0..5 {
        let (running, peak) = (running.clone(), peak.clone());
        ids.push(
            jobs.submit(spec(&format!("j{i}"), 1, 60_000, move || {
                let (running, peak) = (running.clone(), peak.clone());
                async move {
                    let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    sleep(ms(100)).await;
                    running.fetch_sub(1, Ordering::SeqCst);
                    Ok("ok".to_string())
                }
            }))
            .await
            .unwrap(),
        );
    }
    for id in ids {
        jobs.wait(id).await.unwrap();
    }
    assert_eq!(peak.load(Ordering::SeqCst), 2, "одновременно идёт ровно столько заданий, сколько воркеров");
    assert_eq!(started.elapsed(), ms(300), "пять заданий по 100 мс на двух воркерах: три волны");
}

#[tokio::test(start_paused = true)]
async fn higher_priority_goes_first_and_equal_ones_keep_the_order() {
    let (jobs, _runner) = Scheduler::start(config(1));
    let log = Arc::new(Mutex::new(Vec::new()));
    let logged = |name: &'static str, priority: u8, work_ms: u64| {
        let log = log.clone();
        spec(name, priority, 60_000, move || {
            let log = log.clone();
            async move {
                log.lock().unwrap().push(name);
                sleep(ms(work_ms)).await;
                Ok(name.to_string())
            }
        })
    };
    let blocker = jobs.submit(logged("blocker", 5, 50)).await.unwrap();
    settle().await;
    let mut rest = Vec::new();
    for (name, priority) in [("low", 1), ("high", 9), ("mid", 5), ("high2", 9)] {
        rest.push(jobs.submit(logged(name, priority, 10)).await.unwrap());
    }
    jobs.wait(blocker).await.unwrap();
    for id in rest {
        jobs.wait(id).await.unwrap();
    }
    assert_eq!(*log.lock().unwrap(), vec!["blocker", "high", "high2", "mid", "low"], "после блокирующего: сначала высший приоритет, при равенстве по порядку подачи");
}

#[tokio::test(start_paused = true)]
async fn a_timeout_ends_the_job_without_retries() {
    let (jobs, _runner) = Scheduler::start(Config { workers: 1, retries: 3, backoff: ms(100) });
    let calls = Arc::new(AtomicU32::new(0));
    let started = Instant::now();
    let slow = {
        let calls = calls.clone();
        jobs.submit(spec("slow", 1, 100, move || {
            let calls = calls.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                sleep(Duration::from_secs(1)).await;
                Ok("поздно".to_string())
            }
        }))
        .await
        .unwrap()
    };
    assert_eq!(jobs.wait(slow).await.unwrap(), Some(Status::TimedOut), "задание дольше таймаута завершается как TimedOut");
    assert_eq!(started.elapsed(), ms(100), "ровно через таймаут");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "по таймауту повторов нет");
    let next = jobs.submit(sleeper("next", 1, 10)).await.unwrap();
    assert_eq!(jobs.wait(next).await.unwrap(), Some(Status::Done("next".to_string())), "воркер освободился и берёт следующее");
}

#[tokio::test(start_paused = true)]
async fn failures_are_retried_with_doubling_pauses() {
    let (jobs, _runner) = Scheduler::start(Config { workers: 1, retries: 3, backoff: ms(100) });
    let mut updates = jobs.updates();
    let attempts = Arc::new(AtomicU32::new(0));
    let started = Instant::now();
    let flaky = {
        let attempts = attempts.clone();
        jobs.submit(spec("flaky", 1, 60_000, move || {
            let attempts = attempts.clone();
            async move {
                let n = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                if n < 3 { Err(format!("попытка {n}")) } else { Ok("наконец".to_string()) }
            }
        }))
        .await
        .unwrap()
    };
    assert_eq!(jobs.wait(flaky).await.unwrap(), Some(Status::Done("наконец".to_string())));
    assert_eq!(started.elapsed(), ms(300), "паузы 100 мс и 200 мс");
    let mut seen = Vec::new();
    while let Ok(Some((id, status))) = timeout(ms(1), updates.next()).await {
        assert_eq!(id, flaky);
        seen.push(status);
    }
    assert_eq!(
        seen,
        vec![Status::Queued, Status::Running, Status::Retrying(1), Status::Running, Status::Retrying(2), Status::Running, Status::Done("наконец".to_string())],
        "каждое изменение состояния попадает в поток обновлений по порядку"
    );
}

#[tokio::test(start_paused = true)]
async fn retries_run_out_and_the_last_error_wins() {
    let (jobs, _runner) = Scheduler::start(Config { workers: 1, retries: 2, backoff: ms(100) });
    let attempts = Arc::new(AtomicU32::new(0));
    let started = Instant::now();
    let bad = {
        let attempts = attempts.clone();
        jobs.submit(spec("bad", 1, 60_000, move || {
            let attempts = attempts.clone();
            async move {
                let n = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                Err(format!("ошибка {n}"))
            }
        }))
        .await
        .unwrap()
    };
    assert_eq!(jobs.wait(bad).await.unwrap(), Some(Status::Failed("ошибка 3".to_string())), "после двух повторов остаётся последняя ошибка");
    assert_eq!(attempts.load(Ordering::SeqCst), 3, "одна попытка и два повтора");
    assert_eq!(started.elapsed(), ms(300), "100 мс и 200 мс между попытками");
}

#[tokio::test(start_paused = true)]
async fn cancelling_a_queued_job_keeps_it_from_starting() {
    let (jobs, _runner) = Scheduler::start(config(1));
    let started = Arc::new(AtomicBool::new(false));
    let blocker = jobs.submit(sleeper("blocker", 1, 100)).await.unwrap();
    settle().await;
    let queued = {
        let started = started.clone();
        jobs.submit(spec("queued", 1, 60_000, move || {
            let started = started.clone();
            async move {
                started.store(true, Ordering::SeqCst);
                Ok("не должно".to_string())
            }
        }))
        .await
        .unwrap()
    };
    assert_eq!(jobs.status(queued).await.unwrap(), Some(Status::Queued));
    assert!(jobs.cancel(queued).await.unwrap(), "задание в очереди отменяется");
    assert_eq!(jobs.status(queued).await.unwrap(), Some(Status::Cancelled));
    assert!(!jobs.cancel(queued).await.unwrap(), "повторная отмена ничего не делает");
    assert!(!jobs.cancel(777).await.unwrap(), "неизвестный номер не отменяется");
    assert_eq!(jobs.wait(blocker).await.unwrap(), Some(Status::Done("blocker".to_string())));
    settle().await;
    assert!(!started.load(Ordering::SeqCst), "отменённое в очереди задание не должно запускаться");
    assert!(!jobs.cancel(blocker).await.unwrap(), "завершённое не отменяется");
}

#[tokio::test(start_paused = true)]
async fn cancelling_a_running_job_drops_it_and_frees_the_worker_at_once() {
    struct Guard(Arc<AtomicBool>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let (jobs, _runner) = Scheduler::start(config(1));
    let dropped = Arc::new(AtomicBool::new(false));
    let long = {
        let dropped = dropped.clone();
        jobs.submit(spec("long", 1, 60_000, move || {
            let guard = Guard(dropped.clone());
            async move {
                let _guard = guard;
                sleep(Duration::from_secs(3600)).await;
                Ok("поздно".to_string())
            }
        }))
        .await
        .unwrap()
    };
    let after = jobs.submit(sleeper("after", 1, 10)).await.unwrap();
    settle().await;
    assert_eq!(jobs.status(long).await.unwrap(), Some(Status::Running));
    let cancelled_at = Instant::now();
    assert!(jobs.cancel(long).await.unwrap(), "выполняющееся задание отменяется");
    assert_eq!(jobs.wait(long).await.unwrap(), Some(Status::Cancelled));
    assert_eq!(jobs.wait(after).await.unwrap(), Some(Status::Done("after".to_string())), "освободившийся воркер берёт следующее");
    assert_eq!(cancelled_at.elapsed(), ms(10), "следующее задание началось в момент отмены, без ожидания");
    settle().await;
    assert!(dropped.load(Ordering::SeqCst), "future отменённого задания должен быть уничтожен");
}

#[tokio::test(start_paused = true)]
async fn a_panicking_job_fails_and_the_scheduler_lives_on() {
    let (jobs, _runner) = Scheduler::start(Config { workers: 1, retries: 3, backoff: ms(100) });
    let boom = jobs
        .submit(spec("boom", 1, 60_000, || async {
            if true {
                panic!("взорвалось");
            }
            Ok(String::new())
        }))
        .await
        .unwrap();
    assert_eq!(jobs.wait(boom).await.unwrap(), Some(Status::Failed("panic: взорвалось".to_string())), "паника это Failed с текстом, без повторов");
    let next = jobs.submit(sleeper("after", 1, 10)).await.unwrap();
    assert_eq!(jobs.wait(next).await.unwrap(), Some(Status::Done("after".to_string())), "после паники планировщик работает");
}

#[tokio::test(start_paused = true)]
async fn shutdown_gives_running_jobs_the_grace_and_cancels_the_rest() {
    let (jobs, runner) = Scheduler::start(config(1));
    let short = jobs.submit(sleeper("short", 1, 50)).await.unwrap();
    let waiting = jobs.submit(sleeper("waiting", 1, 10)).await.unwrap();
    settle().await;
    let started = Instant::now();
    let report = runner.shutdown(ms(100)).await;
    assert_eq!(report, Report { completed: 1, cancelled: 1 }, "короткое успело за отсрочку, стоявшее в очереди отменено");
    assert_eq!(started.elapsed(), ms(50), "остановка закончилась, когда последнее задание закончилось, не дожидаясь всей отсрочки");
    assert_eq!(jobs.submit(sleeper("late", 1, 1)).await, Err(Closed), "после остановки подача заданий закрыта");
    assert_eq!(jobs.status(short).await, Err(Closed), "и остальные запросы тоже: актор завершён");
    let _ = waiting;
}

#[tokio::test(start_paused = true)]
async fn shutdown_cancels_what_does_not_fit_in_the_grace() {
    let (jobs, runner) = Scheduler::start(config(2));
    let mut updates = jobs.updates();
    let mut ids = Vec::new();
    for (name, work) in [("a", 500), ("b", 20), ("c", 10), ("d", 10)] {
        ids.push(jobs.submit(sleeper(name, 1, work)).await.unwrap());
    }
    settle().await;
    let started = Instant::now();
    let report = runner.shutdown(ms(100)).await;
    assert_eq!(report, Report { completed: 1, cancelled: 3 }, "b успело, a не уложилось в отсрочку, c и d не стартовали");
    assert_eq!(started.elapsed(), ms(100), "ждали ровно отсрочку: a не закончилось");
    let mut last: Vec<(u64, Status)> = Vec::new();
    while let Ok(Some(update)) = timeout(ms(1), updates.next()).await {
        last.push(update);
    }
    for id in &ids {
        let final_status = last.iter().rev().find(|(job, _)| job == id).map(|(_, status)| status.clone());
        let expected = if *id == ids[1] { Status::Done("b".to_string()) } else { Status::Cancelled };
        assert_eq!(final_status, Some(expected), "итоговый статус задания {id} в потоке обновлений");
    }
}

#[tokio::test(start_paused = true)]
async fn nothing_is_left_alive_after_shutdown() {
    let metrics = Handle::current().metrics();
    let baseline = metrics.num_alive_tasks();
    let (jobs, runner) = Scheduler::start(config(3));
    let mut ids = Vec::new();
    for i in 0..20u64 {
        ids.push(jobs.submit(sleeper(&format!("j{i}"), (i % 4) as u8, 30 + i * 10)).await.unwrap());
    }
    settle().await;
    jobs.cancel(ids[1]).await.unwrap();
    jobs.cancel(ids[15]).await.unwrap();
    let report = runner.shutdown(ms(120)).await;
    assert_eq!(report.completed + report.cancelled, 20, "каждое из двадцати заданий либо завершено, либо отменено");
    drop(jobs);
    sleep(ms(100)).await;
    let left = metrics.num_alive_tasks().saturating_sub(baseline);
    assert_eq!(left, 0, "после остановки в рантайме осталось лишних задач: {left}");
}

#[tokio::test(start_paused = true)]
async fn updates_cover_every_job_in_order() {
    let (jobs, _runner) = Scheduler::start(config(1));
    let mut updates = jobs.updates();
    let a = jobs.submit(sleeper("a", 1, 10)).await.unwrap();
    let b = jobs.submit(sleeper("b", 1, 10)).await.unwrap();
    jobs.wait(b).await.unwrap();
    let mut seen = Vec::new();
    while let Ok(Some(update)) = timeout(ms(1), updates.next()).await {
        seen.push(update);
    }
    assert_eq!(
        seen,
        vec![
            (a, Status::Queued),
            (a, Status::Running),
            (b, Status::Queued),
            (a, Status::Done("a".to_string())),
            (b, Status::Running),
            (b, Status::Done("b".to_string())),
        ],
        "общий поток обновлений: изменения всех заданий в порядке, в котором они происходили"
    );
}
