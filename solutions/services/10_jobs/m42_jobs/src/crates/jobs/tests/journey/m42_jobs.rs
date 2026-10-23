use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jobs::{
    Checkpoints, Decision, Failure, Handler, Job, Leader, Leases, Outcome, Periodic, Policy, Queue,
    QueueSpec, Saga, SagaError, SagaLog, Step, StepFuture, Worker, WorkerSpec, now_ms,
    run_checkpointed, run_exclusive, spread,
};
use nats_engine::{Bus, BusConfig, DEFAULT_NATS_URL};
use serde_json::json;

const WAIT: Duration = Duration::from_secs(10);

fn url() -> String {
    std::env::var("RUSTLINGS_NATS_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| DEFAULT_NATS_URL.to_string())
}

fn root() -> String {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.subsec_nanos())
        .unwrap_or(0);
    format!(
        "q{}_{nanos:x}_{}",
        std::process::id() % 100_000,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

async fn bus() -> Bus {
    Bus::connect(BusConfig::new(url()))
        .await
        .expect("NATS курса с JetStream запущен?")
}

async fn js() -> async_nats::jetstream::Context {
    async_nats::jetstream::new(async_nats::connect(url()).await.expect("сырое подключение"))
}

struct Bench {
    root: String,
    queue: Queue,
}

impl Bench {
    async fn open() -> Bench {
        let root = root();
        let spec = QueueSpec {
            stream: format!("{}_JOBS", root.to_ascii_uppercase()),
            prefix: vec![root.clone(), String::from("jobs")],
            dead_stream: format!("{}_JOBS_DEAD", root.to_ascii_uppercase()),
            dead_prefix: vec![root.clone(), String::from("jobs-dead")],
        };
        let queue = Queue::open(bus().await, spec).await.expect("очередь");
        Bench { root, queue }
    }

    fn bucket(&self, name: &str) -> String {
        format!("{}_{name}", self.root.to_ascii_uppercase())
    }

    async fn dead(&self) -> Vec<(String, String)> {
        let stream = js()
            .await
            .get_stream(&self.queue.spec().dead_stream)
            .await
            .expect("мёртвая очередь");
        let mut found = Vec::new();
        let mut sequence = 1;
        while let Ok(message) = stream.get_raw_message(sequence).await {
            let reason = message
                .headers
                .get("X-Error")
                .map(|value| value.as_str().to_string())
                .unwrap_or_default();
            let job: Job = serde_json::from_slice(&message.payload).expect("тело задачи");
            found.push((job.id, reason));
            sequence += 1;
        }
        found
    }

    async fn close(self, buckets: &[&str]) {
        let js = js().await;
        let _ = js.delete_stream(&self.queue.spec().stream).await;
        let _ = js.delete_stream(&self.queue.spec().dead_stream).await;
        for bucket in buckets {
            let _ = js
                .delete_stream(format!("KV_{}", self.bucket(bucket)))
                .await;
        }
    }
}

fn quick() -> Policy {
    Policy {
        first: Duration::from_millis(50),
        cap: Duration::from_millis(200),
        attempts: 3,
    }
}

fn spec(durable: &str) -> WorkerSpec {
    WorkerSpec {
        policy: quick(),
        ack_wait: Duration::from_secs(5),
        ..WorkerSpec::new(durable)
    }
}

type Seen = Arc<Mutex<Vec<(String, Instant)>>>;

fn recording(seen: &Seen, script: fn(&str, usize) -> Outcome) -> Handler {
    let seen = Arc::clone(seen);
    Arc::new(move |job: Job| {
        let seen = Arc::clone(&seen);
        Box::pin(async move {
            let attempt = {
                let mut seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
                seen.push((job.id.clone(), Instant::now()));
                seen.iter().filter(|(id, _)| *id == job.id).count()
            };
            script(&job.id, attempt)
        })
    })
}

async fn until(what: &str, check: impl Fn() -> bool) {
    let began = Instant::now();
    while !check() {
        assert!(began.elapsed() < WAIT, "не дождались: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn count(seen: &Seen, id: &str) -> usize {
    seen.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|(seen, _)| seen == id)
        .count()
}

#[test]
fn the_policy_doubles_the_pause_up_to_the_cap_and_knows_hopeless_jobs() {
    let policy = Policy::default();
    assert_eq!(
        [1, 2, 3, 7, 8, 64, u64::MAX].map(|attempt| policy.backoff(attempt).as_secs()),
        [1, 2, 4, 60, 60, 60, 60],
        "секунда, вдвое, но не дольше минуты и без переполнения"
    );
    let transient = Err(Failure::Transient(String::from("почтовик молчит")));
    assert_eq!(policy.decide(&Ok(()), 1), Decision::Done);
    assert_eq!(
        policy.decide(&transient, 1),
        Decision::Retry(Duration::from_secs(1))
    );
    assert!(
        matches!(policy.decide(&transient, 5), Decision::Dead(reason) if reason.contains("5") && reason.contains("почтовик молчит")),
        "на последней попытке в мёртвую очередь, в причине число попыток и исходная ошибка"
    );
    let permanent = Err(Failure::Permanent(String::from("адреса нет")));
    assert_eq!(
        policy.decide(&permanent, 1),
        Decision::Dead(String::from("адреса нет")),
        "постоянная ошибка сразу в мёртвую очередь"
    );
}

#[tokio::test]
async fn retries_grow_hopeless_jobs_are_buried_and_one_id_is_one_job() {
    let bench = Bench::open().await;
    let seen: Seen = Arc::default();
    let worker = Worker::start(
        &bench.queue,
        spec("workers"),
        recording(&seen, |id, attempt| match id {
            "flaky" if attempt < 3 => Err(Failure::Transient(String::from("таймаут"))),
            "broken" => Err(Failure::Permanent(String::from("адреса нет"))),
            "tired" => Err(Failure::Transient(String::from("всегда таймаут"))),
            _ => Ok(()),
        }),
    )
    .await
    .expect("воркер");
    for id in ["flaky", "broken", "tired", "plain"] {
        let published = bench
            .queue
            .enqueue(&Job::new(id, "mail", json!({})))
            .await
            .expect("задача");
        assert!(!published.duplicate);
    }
    let again = bench
        .queue
        .enqueue(&Job::new("plain", "mail", json!({})))
        .await
        .expect("повтор");
    assert!(
        again.duplicate,
        "одна и та же задача по id ставится один раз"
    );
    until(
        "flaky прошла с третьей попытки",
        || count(&seen, "flaky") == 3,
    )
    .await;
    until("tired исчерпала попытки", || {
        count(&seen, "tired") == 3
    })
    .await;
    let dead = {
        let began = Instant::now();
        loop {
            let dead = bench.dead().await;
            if dead.len() == 2 || began.elapsed() > WAIT {
                break dead;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    let mut ids: Vec<&str> = dead.iter().map(|(id, _)| id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        ["broken", "tired"],
        "в мёртвой очереди только безнадёжные: {dead:?}"
    );
    assert!(
        dead.iter()
            .any(|(id, reason)| id == "broken" && reason == "адреса нет")
            && dead
                .iter()
                .any(|(id, reason)| id == "tired" && reason.contains("всегда таймаут")),
        "у мёртвых писем понятная причина: {dead:?}"
    );
    assert_eq!(count(&seen, "broken"), 1, "постоянную ошибку не повторяют");
    assert_eq!(
        count(&seen, "plain"),
        1,
        "дубль по id не выполнен второй раз"
    );
    let times: Vec<Instant> = seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|(id, _)| id == "flaky")
        .map(|(_, at)| *at)
        .collect();
    let (first, second) = (times[1] - times[0], times[2] - times[1]);
    assert!(
        first >= Duration::from_millis(40) && second >= Duration::from_millis(80),
        "пауза между попытками растёт: {first:?}, {second:?}"
    );
    worker.stop().await;
    bench.close(&[]).await;
}

#[tokio::test]
async fn a_delayed_job_waits_for_its_time_without_blocking_others() {
    let bench = Bench::open().await;
    let seen: Seen = Arc::default();
    let worker = Worker::start(
        &bench.queue,
        spec("delayed"),
        recording(&seen, |_, _| Ok(())),
    )
    .await
    .expect("воркер");
    let at = now_ms() + 700;
    let enqueued = Instant::now();
    bench
        .queue
        .enqueue(&Job::new("later", "remind", json!({})).at(at))
        .await
        .expect("отложенная");
    bench
        .queue
        .enqueue(&Job::new("now", "remind", json!({})))
        .await
        .expect("обычная");
    until("обычная задача выполнена", || {
        count(&seen, "now") == 1
    })
    .await;
    assert_eq!(
        count(&seen, "later"),
        0,
        "отложенная не выполнена раньше срока"
    );
    until("отложенная дождалась срока", || {
        count(&seen, "later") == 1
    })
    .await;
    assert!(
        enqueued.elapsed() >= Duration::from_millis(600),
        "отложенная выполнена не раньше run_at"
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(count(&seen, "later"), 1, "и ровно один раз");
    worker.stop().await;
    bench.close(&[]).await;
}

#[tokio::test]
async fn a_stopped_worker_hands_its_jobs_back_at_once() {
    let bench = Bench::open().await;
    let started = Arc::new(AtomicUsize::new(0));
    let slow: Handler = {
        let started = Arc::clone(&started);
        Arc::new(move |_job: Job| {
            started.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                tokio::time::sleep(Duration::from_secs(60)).await;
                Ok(())
            })
        })
    };
    let first = Worker::start(&bench.queue, spec("shared"), slow)
        .await
        .expect("медленный воркер");
    for n in 0..4 {
        bench
            .queue
            .enqueue(&Job::new(&format!("j{n}"), "slow", json!({})))
            .await
            .expect("задача");
    }
    until(
        "медленный воркер взял задачу",
        || started.load(Ordering::SeqCst) == 1,
    )
    .await;
    let stopped = Instant::now();
    first.stop().await;
    assert!(
        stopped.elapsed() < Duration::from_secs(2),
        "остановка не ждёт конца работы"
    );
    let seen: Seen = Arc::default();
    let second = Worker::start(
        &bench.queue,
        spec("shared"),
        recording(&seen, |_, _| Ok(())),
    )
    .await
    .expect("сосед");
    until(
        "сосед получил все четыре задачи",
        || seen.lock().unwrap_or_else(PoisonError::into_inner).len() == 4,
    )
    .await;
    assert!(
        stopped.elapsed() < Duration::from_secs(4),
        "задачи вернулись сразу, а не через ack_wait в 5 секунд: {:?}",
        stopped.elapsed()
    );
    second.stop().await;
    bench.close(&[]).await;
}

#[test]
fn a_periodic_fires_once_after_a_pause_and_instances_are_spread() {
    let every = Duration::from_millis(1000);
    let mut periodic = Periodic::new("auth-1", every, 10_000);
    let first = periodic.next();
    assert!(
        (10_001..11_000).contains(&first),
        "первое срабатывание в пределах периода: {first}"
    );
    assert!(!periodic.due(first - 1));
    assert!(
        periodic.due(first + 4_500),
        "после долгой паузы срабатывание есть"
    );
    assert!(
        !periodic.due(first + 4_600),
        "но одно, а не по разу за каждый пропущенный тик"
    );
    assert_eq!(
        periodic.next(),
        first + 5_000,
        "следующее на прежней сетке и строго после now"
    );
    let offsets: Vec<u64> = ["auth-1", "auth-2", "auth-3"]
        .iter()
        .map(|name| spread(name, 1000))
        .collect();
    assert!(
        offsets[0] != offsets[1] || offsets[1] != offsets[2],
        "экземпляры не стартуют в одну миллисекунду: {offsets:?}"
    );
    assert_eq!(spread("auth-1", 1000), offsets[0], "сдвиг устойчив");
}

#[tokio::test]
async fn a_lease_has_one_owner_and_a_growing_number() {
    let bench = Bench::open().await;
    let bus = bus().await;
    let leases = Leases::open(&bus, &bench.bucket("LEASES"), Duration::from_millis(1000))
        .await
        .expect("аренды");
    let first = leases
        .acquire("sweep", "a")
        .await
        .expect("брокер")
        .expect("свободна");
    assert!(
        leases
            .acquire("sweep", "b")
            .await
            .expect("брокер")
            .is_none(),
        "занята"
    );
    let renewed = leases.renew(&first).await.expect("брокер").expect("своя");
    assert!(renewed.revision > first.revision, "номер растёт");
    assert!(
        leases.renew(&first).await.expect("брокер").is_none(),
        "старый номер уже не наш"
    );
    assert!(
        !leases.release(&first).await.expect("брокер"),
        "чужую (старую) не отпускает"
    );
    assert!(leases.release(&renewed).await.expect("брокер"));
    let next = leases
        .acquire("sweep", "b")
        .await
        .expect("брокер")
        .expect("снова свободна");
    assert!(next.revision > renewed.revision);
    tokio::time::sleep(Duration::from_millis(2600)).await;
    let taken = leases.acquire("sweep", "c").await.expect("брокер");
    assert!(
        taken.is_some(),
        "замолчавший владелец теряет аренду по сроку"
    );
    let lease = leases
        .acquire("exclusive", "a")
        .await
        .expect("брокер")
        .expect("свободна");
    let worked = run_exclusive(&leases, lease, async {
        tokio::time::sleep(Duration::from_millis(2500)).await;
        42
    })
    .await;
    assert_eq!(
        worked,
        Ok(42),
        "работа дольше срока аренды держит её продлением"
    );
    let lease = leases
        .acquire("stolen", "a")
        .await
        .expect("брокер")
        .expect("свободна");
    let thief = leases.clone();
    let stolen = run_exclusive(&leases, lease.clone(), async move {
        thief.release(&lease).await.expect("брокер");
        thief.acquire("stolen", "b").await.expect("брокер");
        tokio::time::sleep(Duration::from_secs(5)).await;
    })
    .await;
    assert!(stolen.is_err(), "перехваченная аренда останавливает работу");
    bench.close(&["LEASES"]).await;
}

#[tokio::test]
async fn three_instances_sweep_once_per_period() {
    let bench = Bench::open().await;
    let bus = bus().await;
    let every = Duration::from_millis(400);
    let runs = Arc::new(Mutex::new(Vec::new()));
    let mut leaders = Vec::new();
    for name in ["auth-1", "auth-2", "auth-3"] {
        let leases = Leases::open(&bus, &bench.bucket("LEASES_SWEEP"), every * 2)
            .await
            .expect("аренды");
        leaders.push(Leader::new("sweep", name, every, leases, now_ms()));
    }
    let began = Instant::now();
    while began.elapsed() < Duration::from_millis(2100) {
        for leader in &mut leaders {
            let runs = Arc::clone(&runs);
            leader
                .tick(now_ms(), |period| async move {
                    runs.lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(period);
                })
                .await
                .expect("аренда");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let runs = runs.lock().unwrap_or_else(PoisonError::into_inner).clone();
    let mut periods = runs.clone();
    periods.dedup();
    assert_eq!(
        runs, periods,
        "каждый период выполнен одним экземпляром: {runs:?}"
    );
    assert!(runs.len() >= 4, "и периоды не пропущены: {runs:?}");
    bench.close(&["LEASES_SWEEP"]).await;
}

#[tokio::test]
async fn a_checkpointed_mailing_resumes_without_doubles() {
    let bench = Bench::open().await;
    let bus = bus().await;
    let checkpoints = Checkpoints::open(&bus, &bench.bucket("CHECKPOINTS"))
        .await
        .expect("точки");
    let sent = Arc::new(Mutex::new(Vec::new()));
    let fail_at = Arc::new(AtomicU64::new(6));
    let send = |key: String, item: u64| {
        let (sent, fail_at) = (Arc::clone(&sent), Arc::clone(&fail_at));
        async move {
            if item == fail_at.load(Ordering::SeqCst) {
                return Err(Failure::Transient(String::from("почтовик моргнул")));
            }
            sent.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(key);
            Ok(())
        }
    };
    let users: Vec<u64> = (1..=10).collect();
    let first = run_checkpointed("digest.1", &users, &checkpoints, send).await;
    assert!(first.is_err(), "рассылка упала на шестом письме");
    fail_at.store(0, Ordering::SeqCst);
    let second = run_checkpointed("digest.1", &users, &checkpoints, send)
        .await
        .expect("повтор");
    assert_eq!(second, 5, "повтор отправил только остаток");
    let sent = sent.lock().unwrap_or_else(PoisonError::into_inner).clone();
    assert_eq!(sent.len(), 10, "каждому ровно одно письмо: {sent:?}");
    assert_eq!(
        sent[0], "digest.1:1",
        "ключ повтора это задание и получатель"
    );
    bench.close(&["CHECKPOINTS"]).await;
}

struct Probe {
    name: &'static str,
    fail: bool,
    forward: bool,
    log: Arc<Mutex<Vec<String>>>,
}

impl Step for Probe {
    fn name(&self) -> &str {
        self.name
    }

    fn forward_only(&self) -> bool {
        self.forward
    }

    fn run(&self) -> StepFuture<'_> {
        Box::pin(async move {
            self.log
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(format!("run {}", self.name));
            match self.fail {
                true => Err(Failure::Transient(format!("{} упал", self.name))),
                false => Ok(()),
            }
        })
    }

    fn compensate(&self) -> StepFuture<'_> {
        Box::pin(async move {
            self.log
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(format!("undo {}", self.name));
            Ok(())
        })
    }
}

#[tokio::test]
async fn a_saga_rolls_back_in_reverse_or_retries_after_the_pivot() {
    let bench = Bench::open().await;
    let bus = bus().await;
    let log = SagaLog::open(&bus, &bench.bucket("SAGAS"))
        .await
        .expect("журнал саги");
    let steps = Arc::new(Mutex::new(Vec::new()));
    let probe = |name, fail, forward| Probe {
        name,
        fail,
        forward,
        log: Arc::clone(&steps),
    };
    let failed = Saga::new("delete.7", &log)
        .step(probe("freeze", false, false))
        .step(probe("hide", false, false))
        .step(probe("profile", true, false))
        .step(probe("posts", false, true))
        .run()
        .await;
    assert!(
        matches!(&failed, Err(SagaError::RolledBack { failed, stuck, .. }) if failed == "profile" && stuck.is_empty()),
        "{failed:?}"
    );
    assert_eq!(
        *steps.lock().unwrap_or_else(PoisonError::into_inner),
        [
            "run freeze",
            "run hide",
            "run profile",
            "undo hide",
            "undo freeze"
        ],
        "упавший шаг не откатывается, сделанные откатываются в обратном порядке"
    );
    steps.lock().unwrap_or_else(PoisonError::into_inner).clear();
    let pivot = Saga::new("delete.8", &log)
        .step(probe("freeze", false, false))
        .step(probe("posts", true, true))
        .run()
        .await;
    assert!(
        matches!(pivot, Err(SagaError::Retry(..))),
        "после необратимого шага только вперёд: {pivot:?}"
    );
    steps.lock().unwrap_or_else(PoisonError::into_inner).clear();
    let resumed = Saga::new("delete.8", &log)
        .step(probe("freeze", false, false))
        .step(probe("posts", false, true))
        .run()
        .await;
    assert_eq!(resumed, Ok(()));
    assert_eq!(
        *steps.lock().unwrap_or_else(PoisonError::into_inner),
        ["run posts"],
        "перезапуск продолжает с недоделанного шага: сделанное по журналу не повторяется"
    );
    bench.close(&["SAGAS"]).await;
}
