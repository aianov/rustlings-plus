use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use service_runtime::{
    CancellationToken, Outcome, Restart, Shutdown, ShutdownReport, StageReport, TaskTracker,
    supervise,
};
use tokio::time::{Instant, sleep, timeout};

const DEADLINE: Duration = Duration::from_secs(60);

async fn within<T>(what: &str, work: impl Future<Output = T>) -> T {
    timeout(DEADLINE, work)
        .await
        .unwrap_or_else(|_| panic!("{what}: не уложилось в {DEADLINE:?}, похоже на зависание"))
}

fn shareable<T: Clone + Send + Sync + 'static>() {}

#[tokio::test]
async fn token_is_seen_by_every_clone_and_late_listeners() {
    shareable::<CancellationToken>();
    shareable::<TaskTracker>();
    let token = CancellationToken::new();
    let clone = token.clone();
    assert!(!clone.is_cancelled(), "новый токен не отменён");
    let waiter = {
        let token = token.clone();
        tokio::spawn(async move { token.cancelled().await })
    };
    token.cancel();
    within("ожидающий токена", waiter)
        .await
        .expect("ожидающий не падает");
    assert!(clone.is_cancelled(), "отмену видит любой клон");
    within("поздний слушатель", clone.cancelled()).await;
}

#[tokio::test]
async fn children_follow_the_parent_but_not_back() {
    let root = CancellationToken::new();
    let child = root.child();
    let grandchild = child.child();
    let sibling = root.child();
    child.cancel();
    assert!(
        child.is_cancelled() && grandchild.is_cancelled(),
        "отмена идёт вниз по дереву"
    );
    assert!(
        !root.is_cancelled() && !sibling.is_cancelled(),
        "отмена ребёнка не трогает родителя и соседей"
    );
    within("внук слышит отмену деда", async {
        let waiting = sibling.child();
        let wait = tokio::spawn(async move { waiting.cancelled().await });
        root.cancel();
        wait.await.expect("ожидающий не падает");
    })
    .await;
    assert!(sibling.is_cancelled());
}

#[tokio::test(start_paused = true)]
async fn tracker_waits_for_every_task_after_close() {
    let tracker = TaskTracker::new();
    let done = Arc::new(AtomicU32::new(0));
    for n in 1..=3u64 {
        let done = Arc::clone(&done);
        tracker.spawn(async move {
            sleep(Duration::from_secs(n)).await;
            done.fetch_add(1, Ordering::SeqCst);
        });
    }
    assert_eq!(tracker.len(), 3, "учёт видит три задачи");
    tracker.close();
    let started = Instant::now();
    within("ожидание учёта", tracker.wait()).await;
    assert_eq!(done.load(Ordering::SeqCst), 3, "wait дождался всех");
    assert_eq!(
        started.elapsed(),
        Duration::from_secs(3),
        "и не дольше самой долгой"
    );
    assert!(tracker.is_empty());
}

#[tokio::test(start_paused = true)]
async fn tracker_counts_aborted_tasks_as_gone() {
    let tracker = TaskTracker::new();
    tracker.spawn(async { sleep(Duration::from_secs(3600)).await });
    tracker.close();
    assert_eq!(tracker.abort_all(), 1, "abort_all отменяет живые задачи");
    within("учёт после abort", tracker.wait()).await;
    assert!(tracker.is_empty(), "отменённая задача тоже уходит из учёта");
}

#[tokio::test(start_paused = true)]
async fn supervisor_restarts_with_a_pause_and_gives_up() {
    let attempts = Arc::new(AtomicU32::new(0));
    let make = {
        let attempts = Arc::clone(&attempts);
        move || {
            let attempts = Arc::clone(&attempts);
            async move {
                if attempts.fetch_add(1, Ordering::SeqCst) < 2 {
                    std::panic::resume_unwind(Box::new("упала"));
                }
            }
        }
    };
    let policy = Restart {
        pause: Duration::from_secs(5),
        max_restarts: 3,
    };
    let started = Instant::now();
    let outcome = within(
        "супервизор",
        supervise(make, CancellationToken::new(), policy),
    )
    .await;
    assert_eq!(
        outcome,
        Outcome::Finished { restarts: 2 },
        "две паники, на третий раз вышло"
    );
    assert_eq!(
        started.elapsed(),
        Duration::from_secs(10),
        "две паузы по 5 с"
    );
    let always = || async { std::panic::resume_unwind(Box::new("снова")) };
    let gave_up = within(
        "супервизор",
        supervise(
            always,
            CancellationToken::new(),
            Restart {
                pause: Duration::from_secs(1),
                max_restarts: 2,
            },
        ),
    )
    .await;
    assert_eq!(
        gave_up,
        Outcome::GaveUp { attempts: 3 },
        "лимит перезапусков: три попытки и всё"
    );
}

#[tokio::test(start_paused = true)]
async fn supervisor_stops_on_cancel() {
    let token = CancellationToken::new();
    let forever = || async { sleep(Duration::from_secs(3600)).await };
    let run = tokio::spawn(supervise(
        forever,
        token.clone(),
        Restart {
            pause: Duration::from_secs(1),
            max_restarts: 10,
        },
    ));
    sleep(Duration::from_secs(1)).await;
    token.cancel();
    let outcome = within("супервизор", run)
        .await
        .expect("супервизор не падает");
    assert_eq!(outcome, Outcome::Cancelled { restarts: 0 });
}

#[tokio::test(start_paused = true)]
async fn shutdown_goes_stage_by_stage_and_aborts_the_late() {
    let mut shutdown = Shutdown::new();
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));
    let stages = [
        shutdown.stage("sources"),
        shutdown.stage("handlers"),
        shutdown.stage("trends"),
    ];
    for stage in &stages {
        let token = stage.token.clone();
        let order = Arc::clone(&order);
        let name = stage.name;
        stage.tasks.spawn(async move {
            token.cancelled().await;
            order
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(name);
        });
    }
    stages[2]
        .tasks
        .spawn(async { sleep(Duration::from_secs(3600)).await });
    let started = Instant::now();
    let report: ShutdownReport = within("остановка", shutdown.close(Duration::from_secs(2))).await;
    assert_eq!(
        report.stages,
        [
            StageReport {
                name: "sources",
                aborted: 0
            },
            StageReport {
                name: "handlers",
                aborted: 0
            },
            StageReport {
                name: "trends",
                aborted: 1
            },
        ],
        "стадии гасятся по порядку, упрямую задачу добивает abort по дедлайну"
    );
    assert_eq!(report.aborted(), 1);
    assert_eq!(report.order(), ["sources", "handlers", "trends"]);
    assert_eq!(
        *order
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
        ["sources", "handlers", "trends"],
        "задачи стадии слышат отмену только когда до неё дошла очередь"
    );
    assert_eq!(
        started.elapsed(),
        Duration::from_secs(2),
        "общий дедлайн один на всю остановку"
    );
    assert!(
        shutdown.token().is_cancelled(),
        "после close отменён и корень"
    );
}
