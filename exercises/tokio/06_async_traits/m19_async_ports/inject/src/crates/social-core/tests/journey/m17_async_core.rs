use std::collections::HashSet;
use std::fs;
use std::future::Future;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant as WallClock};

use contracts::ParseError;
use social_core::{
    App, AppConfig, AppError, CommandError, DomainError, Error, ImportError, Imported,
    IntoResponse, MemNetwork, Notification, Notifier, Notifiers, Op, Shards, Stats, UserId,
    Visibility, execute_line, report,
};
use test_support::FakeClock;
use tokio::runtime::Builder;
use tokio::task::JoinSet;
use tokio::time::timeout;

type Shared = App;

const DEADLINE: Duration = Duration::from_secs(10);

const SHORT: Duration = Duration::from_millis(200);

async fn within<T>(what: &str, work: impl Future<Output = T>) -> T {
    timeout(DEADLINE, work)
        .await
        .unwrap_or_else(|_| panic!("{what}: не уложилось в {DEADLINE:?}, похоже на зависание"))
}

async fn join_all<T: Send + 'static>(what: &str, mut tasks: JoinSet<T>) -> Vec<T> {
    within(what, async move {
        let mut done = Vec::new();
        while let Some(result) = tasks.join_next().await {
            done.push(result.unwrap_or_else(|e| panic!("{what}: задача упала: {e}")));
        }
        done
    })
    .await
}

fn start(config: AppConfig) -> Shared {
    App::start(MemNetwork::new(), Notifiers::new(), config)
        .expect("ядро запускается внутри рантайма")
}

fn with_notifier(notifier: Arc<dyn Notifier>, config: AppConfig) -> Shared {
    let mut notifiers = Notifiers::new();
    notifiers.add(notifier);
    App::start(MemNetwork::new(), notifiers, config).expect("ядро запускается внутри рантайма")
}

fn quick() -> AppConfig {
    AppConfig {
        op_timeout: SHORT,
        heavy_limit: 1,
        ..AppConfig::default()
    }
}

async fn user(net: &Shared, name: &str) -> UserId {
    net.register(name)
        .await
        .unwrap_or_else(|e| panic!("{name} должен регистрироваться: {e}"))
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}

impl Gate {
    fn wait(&self) -> bool {
        let open = lock(&self.open);
        let (open, _) = self
            .changed
            .wait_timeout_while(open, Duration::from_secs(5), |open| !*open)
            .unwrap_or_else(PoisonError::into_inner);
        *open
    }

    fn open(&self) {
        *lock(&self.open) = true;
        self.changed.notify_all();
    }
}

struct Opener(Arc<Gate>);

impl Drop for Opener {
    fn drop(&mut self) {
        self.0.open();
    }
}

struct HeldNewPosts(Arc<Gate>);

impl Notifier for HeldNewPosts {
    fn notify(&self, _user: UserId, notification: Notification) {
        if matches!(notification, Notification::NewPost { .. }) {
            self.0.wait();
        }
    }
}

struct PanicOnFollow;

impl Notifier for PanicOnFollow {
    fn notify(&self, _user: UserId, notification: Notification) {
        if matches!(notification, Notification::Followed { .. }) {
            std::panic::resume_unwind(Box::new("уведомитель с ошибкой упал на подписке"));
        }
    }
}

#[test]
fn app_starts_only_inside_a_runtime() {
    let outside = App::new(MemNetwork::new());
    assert!(
        matches!(outside, Err(Error::NoRuntime)),
        "вне рантайма tokio App::new отдаёт Error::NoRuntime, а не паникует: {:?}",
        outside.err()
    );
    let error = Error::NoRuntime;
    assert_eq!(error.code(), "internal", "NoRuntime это internal");
    assert_eq!(
        error.to_string(),
        "ядро запускается только внутри рантайма tokio"
    );
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("рантайм строится");
    let _entered = runtime.enter();
    assert!(
        App::new(MemNetwork::new()).is_ok(),
        "внутри контекста рантайма (enter) App запускается"
    );
}

fn send<T: Send>(_: T) {}

fn shareable<T: Send + Sync + Clone + 'static>() {}

#[tokio::test]
async fn app_futures_are_send() {
    shareable::<Shared>();
    let net = start(AppConfig::default());
    let id = UserId(1);
    let post = social_core::PostId(1);
    let comment = social_core::CommentId(1);
    send(net.register("alice"));
    send(net.publish(id, "текст", Visibility::Public));
    send(net.follow(id, id));
    send(net.like(id, post));
    send(net.mute(id, id));
    send(net.delete_post(id, post));
    send(net.comment(id, post, "текст"));
    send(net.reply(id, comment, "текст"));
    send(net.send_message(id, id, "текст"));
    send(net.inbox(id));
    send(net.flush());
    send(net.refresh_trends());
    send(net.import(String::new()));
    send(net.import_file("seed.txt"));
    send(net.run_heavy(Op::Job("проверка"), || 1));
    send(execute_line("feed alice", &net));
}

#[test]
fn shards_spread_keys_and_keep_counts() {
    let shards: Shards<u64, u64> = Shards::new(8);
    assert_eq!(shards.count(), 8);
    assert_eq!(
        Shards::<u64, u64>::new(0).count(),
        1,
        "ноль шардов это один шард"
    );
    assert_eq!(
        Shards::<u64, u64>::default().count(),
        social_core::DEFAULT_SHARDS
    );
    assert!(shards.is_empty());
    let used: HashSet<usize> = (0..1000).map(|key| shards.shard_of(&key)).collect();
    assert!(
        used.len() >= 4 && used.iter().all(|&shard| shard < 8),
        "1000 ключей ложатся на разные шарды по хешу, а легли на {used:?}"
    );
    assert_eq!(
        shards.shard_of(&42),
        shards.shard_of(&42),
        "один ключ всегда в одном шарде"
    );
    for key in 0..100 {
        shards.with(&key, |shard| shard.insert(key, key * 2));
    }
    assert_eq!(shards.len(), 100, "len складывает все шарды");
    assert_eq!(shards.get(&21), Some(42), "get отдаёт копию значения");
    assert_eq!(shards.get(&1000), None);
    let removed = shards.with(&21, |shard| shard.remove(&21));
    assert_eq!(removed, Some(42), "with отдаёт то, что вернуло замыкание");
    assert_eq!(shards.len(), 99);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shards_take_many_writers_at_once() {
    let shards: Arc<Shards<u64, u64>> = Arc::new(Shards::new(4));
    let mut writers = JoinSet::new();
    for writer in 0..8u64 {
        let shards = Arc::clone(&shards);
        writers.spawn(async move {
            for n in 0..1000u64 {
                let key = (writer + n) % 16;
                shards.with(&key, |shard| *shard.entry(key).or_default() += 1);
                if n % 100 == 0 {
                    tokio::task::yield_now().await;
                }
            }
        });
    }
    join_all("8 писателей по 1000 записей", writers).await;
    let total: u64 = (0..16).filter_map(|key| shards.get(&key)).sum();
    assert_eq!(total, 8000, "ни одна запись в шарды не потерялась");
    assert_eq!(shards.len(), 16);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inboxes_fill_from_many_publishers() {
    let net = start(AppConfig::default());
    let mut authors = Vec::new();
    for a in 0..10 {
        let author = user(&net, &format!("author{a}")).await;
        let mut readers = Vec::new();
        for r in 0..3 {
            let reader = user(&net, &format!("reader{a}_{r}")).await;
            net.follow(reader, author).await.expect("подписка проходит");
            readers.push(reader);
        }
        authors.push((author, readers));
    }
    let mut writers = JoinSet::new();
    for (author, _) in &authors {
        let net = net.clone();
        let author = *author;
        writers.spawn(async move {
            for n in 0..5 {
                net.publish(author, &format!("пост {n}"), Visibility::Public)
                    .await
                    .expect("публикация проходит");
            }
        });
    }
    join_all("10 авторов по 5 постов", writers).await;
    for (author, readers) in &authors {
        for reader in readers {
            let inbox = within("inbox читателя", net.inbox(*reader))
                .await
                .expect("рассылка доходит вовремя");
            let posts = inbox
                .iter()
                .filter(
                    |n| matches!(n, Notification::NewPost { author: from, .. } if from == author),
                )
                .count();
            assert_eq!(
                posts, 5,
                "читатель получил все 5 постов своего автора: {inbox:?}"
            );
        }
    }
    assert_eq!(
        net.stats(),
        Stats {
            posts: 50,
            notifications: 30 + 150,
            trend_refreshes: 0,
        },
        "30 подписок и 150 новых постов"
    );
}

struct Probe {
    running: AtomicUsize,
    peak: AtomicUsize,
}

impl Probe {
    fn new() -> Arc<Probe> {
        Arc::new(Probe {
            running: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        })
    }

    fn job(
        self: &Arc<Probe>,
        release: mpsc::Receiver<()>,
    ) -> impl FnOnce() -> bool + Send + 'static {
        let probe = Arc::clone(self);
        move || {
            let now = probe.running.fetch_add(1, Ordering::SeqCst) + 1;
            probe.peak.fetch_max(now, Ordering::SeqCst);
            let released = release.recv_timeout(Duration::from_secs(5)).is_ok();
            probe.running.fetch_sub(1, Ordering::SeqCst);
            released
        }
    }

    async fn wait_running(&self, n: usize) {
        let deadline = WallClock::now() + Duration::from_secs(5);
        while self.running.load(Ordering::SeqCst) < n {
            assert!(
                WallClock::now() < deadline,
                "за 5 с не набралось {n} тяжёлых работ одновременно"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn heavy_jobs_respect_the_limit() {
    let net = start(AppConfig {
        heavy_limit: 2,
        ..AppConfig::default()
    });
    let probe = Probe::new();
    let mut jobs = JoinSet::new();
    let mut releases = Vec::new();
    for _ in 0..6 {
        let (release, wait) = mpsc::channel();
        releases.push(release);
        let net = net.clone();
        let job = probe.job(wait);
        jobs.spawn(async move { net.run_heavy(Op::Job("тяжёлая работа"), job).await });
    }
    probe.wait_running(2).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        probe.running.load(Ordering::SeqCst),
        2,
        "heavy_limit = 2: третья тяжёлая работа ждёт разрешения Semaphore, а не стартует"
    );
    for release in releases {
        let _ = release.send(());
    }
    let results = join_all("шесть тяжёлых работ", jobs).await;
    assert!(
        results.iter().all(|result| *result == Ok(true)),
        "все шесть работ выполнились: {results:?}"
    );
    assert_eq!(
        probe.peak.load(Ordering::SeqCst),
        2,
        "одновременно шло не больше двух работ и хотя бы раз ровно две"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn heavy_job_waits_for_a_permit_until_timeout() {
    let net = start(quick());
    let probe = Probe::new();
    let (release, wait) = mpsc::channel();
    let first = {
        let net = net.clone();
        let job = probe.job(wait);
        tokio::spawn(async move { net.run_heavy(Op::Job("первая"), job).await })
    };
    probe.wait_running(1).await;
    let started = WallClock::now();
    let second = net.run_heavy(Op::Job("вторая"), || 2).await;
    assert_eq!(
        second,
        Err(AppError::Timeout {
            op: Op::Job("вторая"),
            after: SHORT,
        }),
        "разрешение занято, а ждать его можно не дольше op_timeout"
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "таймаут сработал через {:?}, а должен через op_timeout = {SHORT:?}",
        started.elapsed()
    );
    let _ = release.send(());
    let first = within("первая работа", first).await;
    assert!(
        first.is_ok(),
        "задача первой работы не падает, а её результат уже не важен: ждали дольше op_timeout"
    );
    let deadline = WallClock::now() + Duration::from_secs(5);
    while probe.running.load(Ordering::SeqCst) > 0 && WallClock::now() < deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(
        net.run_heavy(Op::Job("третья"), || 3).await,
        Ok(3),
        "когда разрешение вернулось, следующая работа проходит"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slow_heavy_job_times_out_but_keeps_its_permit() {
    let net = start(quick());
    let probe = Probe::new();
    let (release, wait) = mpsc::channel();
    let slow = net.run_heavy(Op::Job("медленная"), probe.job(wait)).await;
    assert_eq!(
        slow,
        Err(AppError::Timeout {
            op: Op::Job("медленная"),
            after: SHORT,
        }),
        "работа дольше op_timeout: ждать её результат перестали"
    );
    assert_eq!(
        probe.running.load(Ordering::SeqCst),
        1,
        "а сама работа ещё идёт в spawn_blocking, её не прервать"
    );
    assert_eq!(
        net.run_heavy(Op::Job("следующая"), || 1).await,
        Err(AppError::Timeout {
            op: Op::Job("следующая"),
            after: SHORT,
        }),
        "пока работа идёт, она держит разрешение: permit уезжает в spawn_blocking \
         (acquire_owned), иначе таймауты тихо превысят heavy_limit"
    );
    let _ = release.send(());
    let deadline = WallClock::now() + Duration::from_secs(5);
    while probe.running.load(Ordering::SeqCst) > 0 && WallClock::now() < deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(
        net.run_heavy(Op::Job("после"), || 7).await,
        Ok(7),
        "работа закончилась, разрешение вернулось"
    );
}

#[tokio::test]
async fn panicking_heavy_job_is_an_error() {
    let net = start(AppConfig {
        heavy_limit: 1,
        ..AppConfig::default()
    });
    let job =
        || -> u32 { std::panic::resume_unwind(Box::new("тяжёлая работа упала")) };
    assert_eq!(
        net.run_heavy(Op::Job("падающая"), job).await,
        Err(AppError::Panicked(Op::Job("падающая"))),
        "паника в spawn_blocking становится AppError::Panicked, процесс живёт дальше"
    );
    assert_eq!(
        net.run_heavy(Op::Job("после паники"), || 5).await,
        Ok(5),
        "разрешение упавшей работы вернулось"
    );
}

const SEED: &str = "register alice
register bob
register carol

follow bob alice
follow carol alice
post alice Первый пост #rust
post alice Второй
post bob Привет от bob
";

#[tokio::test]
async fn import_builds_the_network() {
    let clock = FakeClock::new(1_700_000_000);
    let net = App::start(
        MemNetwork::new().with_clock(clock),
        Notifiers::new(),
        AppConfig::default(),
    )
    .expect("ядро запускается внутри рантайма");
    let imported = within("импорт", net.import(String::from(SEED)))
        .await
        .expect("импорт проходит");
    assert_eq!(
        imported,
        Imported {
            users: 3,
            follows: 2,
            posts: 3,
        },
        "импорт считает, что сделал"
    );
    let alice = net.find_user("alice").await.map(|user| user.id());
    let followers = match alice {
        Some(alice) => Some(net.follow_counts(alice).await.followers),
        None => None,
    };
    let (users, posts) = (net.user_count().await, net.post_count().await);
    assert_eq!((users, posts, followers), (3, 3, Some(2)));
    let feed = within("лента bob", execute_line("feed bob", &net))
        .await
        .expect("лента строится");
    assert!(
        feed.contains("Привет от bob") && feed.contains("Первый пост #rust"),
        "в ленте bob свой пост и посты alice, на которую он подписан:\n{feed}"
    );
    let inbox = within("inbox bob", net.inbox(UserId(2)))
        .await
        .expect("рассылка доходит вовремя");
    assert_eq!(
        inbox
            .iter()
            .filter(|n| matches!(n, Notification::NewPost { .. }))
            .count(),
        2,
        "подписчик alice узнал о двух её импортированных постах: импорт публикует как обычно"
    );
}

#[tokio::test]
async fn import_checks_the_whole_file_first() {
    let net = start(AppConfig::default());
    let result = net
        .import(String::from("register alice\nregister bob\nfollow bob\n"))
        .await;
    assert_eq!(
        result,
        Err(AppError::Import(ImportError::Parse {
            line: 3,
            error: ParseError::MissingArgument("на кого"),
        })),
        "разбор идёт до первой записи: ошибка в строке 3"
    );
    assert_eq!(
        net.user_count().await,
        0,
        "файл с ошибкой разбора не импортируется даже частично"
    );
    assert_eq!(
        net.import(String::from("register alice\nlike alice 1\n"))
            .await,
        Err(AppError::Import(ImportError::Unsupported { line: 2 })),
        "в импорте только register, follow и post"
    );
}

#[tokio::test]
async fn import_reports_unknown_names_by_line() {
    let net = start(AppConfig::default());
    assert_eq!(
        net.import(String::from("register bob\nfollow bob ghost\n"))
            .await,
        Err(AppError::Import(ImportError::UnknownName {
            line: 2,
            name: String::from("ghost"),
        })),
        "подписка на незнакомый ник: строка и ник"
    );
    assert_eq!(
        net.import(String::from("register zoe\nregister zoe\n"))
            .await,
        Err(AppError::Import(ImportError::Record {
            line: 2,
            error: DomainError::NameTaken(String::from("zoe")),
        })),
        "ошибка домена приходит с номером строки"
    );
}

#[tokio::test]
async fn first_failed_record_stops_the_import() {
    let net = start(AppConfig {
        import_window: 4,
        ..AppConfig::default()
    });
    let mut text = String::new();
    for n in 1..=300 {
        if n == 20 {
            text.push_str("register user5\n");
        } else {
            text.push_str(&format!("register user{n}\n"));
        }
    }
    let result = within("импорт 300 записей", net.import(text)).await;
    assert_eq!(
        result,
        Err(AppError::Import(ImportError::Record {
            line: 20,
            error: DomainError::NameTaken(String::from("user5")),
        })),
        "строка 20 повторяет ник из строки 5"
    );
    let users = net.user_count().await;
    assert!(
        users < 19 + 4,
        "после ошибки остальные записи отменяются: в полёте не больше import_window = 4, \
         поэтому до ошибки успевает максимум 22 записи, а зарегистрировано {users}"
    );
}

#[tokio::test]
async fn panic_in_a_record_becomes_an_import_error() {
    let net = with_notifier(Arc::new(PanicOnFollow), AppConfig::default());
    let result = within(
        "импорт с падающим уведомителем",
        net.import(String::from(
            "register alice\nregister bob\nfollow bob alice\n",
        )),
    )
    .await;
    assert_eq!(
        result,
        Err(AppError::Import(ImportError::Panicked { line: 3 })),
        "паника в задаче записи становится ошибкой импорта со строкой, а не роняет процесс"
    );
    assert!(
        net.register("carol").await.is_ok(),
        "после паники ядро работает дальше: отравленный Mutex не мешает"
    );
}

fn seed_file(name: &str, text: &str) -> String {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("m17");
    fs::create_dir_all(&dir).expect("временная папка создаётся");
    let path = dir.join(name);
    fs::write(&path, text).expect("файл импорта пишется");
    path.display().to_string()
}

#[tokio::test]
async fn import_file_reads_from_disk() {
    let net = start(AppConfig::default());
    let path = seed_file("seed.txt", SEED);
    let imported = within("импорт файла", net.import_file(path.as_str()))
        .await
        .expect("импорт файла проходит");
    assert_eq!(imported.users, 3);
    let missing = net.import_file("нет/такого/файла.txt").await;
    assert_eq!(
        missing,
        Err(AppError::Import(ImportError::Read {
            path: "нет/такого/файла.txt".into(),
            kind: io::ErrorKind::NotFound,
        })),
        "нет файла: ImportError::Read с путём и видом ошибки"
    );
}

#[tokio::test]
async fn import_command_reports_counts() {
    let net = start(AppConfig::default());
    let path = seed_file(
        "command.txt",
        "register alice\nregister bob\nfollow bob alice\npost alice Привет\n",
    );
    assert_eq!(
        within("import", execute_line(&format!("import {path}"), &net)).await,
        Ok(String::from(
            "импортировано: пользователей 2, подписок 1, постов 1"
        )),
        "команда import печатает итог"
    );
    let broken = seed_file("broken.txt", "register carol\nregister alice\n");
    let error = within("import", execute_line(&format!("import {broken}"), &net))
        .await
        .expect_err("alice уже есть");
    assert_eq!(
        error.to_string(),
        "импорт остановлен: строка 2: ник alice уже занят",
        "ошибка импорта печатается всей цепочкой"
    );
    assert_eq!(error.into_response().status, 422, "плохой файл это 422");
}

#[tokio::test(start_paused = true)]
async fn trends_refresh_on_the_ticker() {
    let net = start(AppConfig {
        trends_every: Duration::from_secs(60),
        ..AppConfig::default()
    });
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    let post = net
        .publish(alice, "в тренды", Visibility::Public)
        .await
        .expect("пост публикуется");
    net.like(bob, post).await.expect("лайк проходит");
    assert!(
        net.trends(10).is_empty(),
        "trends читает готовую сводку, а её ещё никто не пересчитал"
    );
    tokio::time::sleep(Duration::from_secs(61)).await;
    wait_refreshes(
        &net,
        1,
        "фоновая задача пересчитывает тренды раз в trends_every",
    )
    .await;
    let trends = net.trends(10);
    assert_eq!(
        trends.iter().map(|trend| trend.post).collect::<Vec<_>>(),
        [post],
        "через минуту фоновый пересчёт по interval положил пост в тренды"
    );
}

async fn wait_refreshes(net: &Shared, n: u64, why: &str) {
    let deadline = WallClock::now() + Duration::from_secs(5);
    while net.stats().trend_refreshes < n {
        assert!(
            WallClock::now() < deadline,
            "за 5 с не дождались {n} пересчётов трендов, было {}: {why}",
            net.stats().trend_refreshes
        );
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn ticker_skips_missed_ticks() {
    let net = start(AppConfig {
        trends_every: Duration::from_secs(60),
        ..AppConfig::default()
    });
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(301)).await;
    wait_refreshes(&net, 1, "после простоя тикер срабатывает сразу").await;
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        net.stats().trend_refreshes,
        1,
        "ядро простояло пять тиков: со Skip пересчёт идёт один раз, а не пачкой из пяти (Burst)"
    );
    tokio::time::advance(Duration::from_secs(58)).await;
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        net.stats().trend_refreshes,
        1,
        "до следующей отметки сетки тиков ещё рано"
    );
    tokio::time::advance(Duration::from_millis(1500)).await;
    wait_refreshes(
        &net,
        2,
        "со Skip следующий тик по сетке, на 360 с; Delay сдвинул бы его на 361 с",
    )
    .await;
    assert_eq!(
        net.stats().trend_refreshes,
        2,
        "со Skip следующий тик приходит по сетке (360 с), а не через минуту после позднего (Delay)"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn flush_gives_up_after_op_timeout() {
    let gate = Arc::new(Gate::default());
    let _opener = Opener(gate.clone());
    let net = with_notifier(Arc::new(HeldNewPosts(gate.clone())), quick());
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    net.follow(bob, alice).await.expect("подписка проходит");
    net.publish(alice, "застрянет", Visibility::Public)
        .await
        .expect("пост публикуется");
    let started = WallClock::now();
    assert_eq!(
        net.flush().await,
        Err(AppError::Timeout {
            op: Op::Flush,
            after: SHORT,
        }),
        "доставка висит: flush ждёт не дольше op_timeout"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(
        net.inbox(bob).await.is_err(),
        "inbox сначала ждёт рассылку и отдаёт её таймаут"
    );
    gate.open();
    assert_eq!(
        within("flush после открытия", net.flush()).await,
        Ok(()),
        "доставка отвисла, flush проходит"
    );
}

#[test]
fn app_errors_explain_themselves() {
    assert_eq!(Op::Import.to_string(), "импорт");
    assert_eq!(Op::Trends.to_string(), "пересчёт трендов");
    assert_eq!(Op::Flush.to_string(), "доставка уведомлений");
    assert_eq!(Op::Job("отчёт").to_string(), "отчёт");
    let late = AppError::Timeout {
        op: Op::Flush,
        after: Duration::from_millis(200),
    };
    assert_eq!(
        late.to_string(),
        "доставка уведомлений: превышено время ожидания (200ms)"
    );
    assert_eq!(
        AppError::Panicked(Op::Trends).to_string(),
        "пересчёт трендов: задача упала с паникой"
    );
    let import = AppError::Import(ImportError::Record {
        line: 4,
        error: DomainError::SelfFollow,
    });
    assert_eq!(
        report(&import),
        "импорт остановлен: строка 4: нельзя подписаться на самого себя",
        "AppError отдаёт причину через source"
    );
    assert_eq!(
        CommandError::from(AppError::Domain(DomainError::SelfFollow)),
        CommandError::Domain(DomainError::SelfFollow),
        "доменная ошибка ядра в шине остаётся CommandError::Domain"
    );
    assert_eq!(
        CommandError::from(late.clone()).into_response().status,
        503,
        "таймаут ядра в шине это 503"
    );
    assert_eq!(
        Error::from(late).code(),
        "unavailable",
        "таймаут ядра на проводе это unavailable"
    );
    assert_eq!(Error::from(import).code(), "rejected");
    assert!(
        matches!(
            Error::from(AppError::Domain(DomainError::SelfFollow)),
            Error::Domain(DomainError::SelfFollow)
        ),
        "доменная ошибка ядра на проводе остаётся Error::Domain"
    );
}
