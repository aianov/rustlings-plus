use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use service_runtime::{
    Background, CHECK_LIMIT, CancellationToken, Criticality, Halt, Health, Lifecycle, Policy,
    Problem, Restart, Secret, Signal, Signals, Source, Step, Timing, Trail, parse_dotenv, wait_for,
};
use tokio::time::{Instant, sleep, timeout};

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("m34")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("временная папка создаётся");
    dir
}

#[test]
fn dotenv_understands_comments_export_and_quotes() {
    let text = "\
# настройки разработки
USERS_GRPC_ADDR=127.0.0.1:50052

export CERTS_DIR = certs
GREETING=\"привет # это не комментарий\"
SINGLE='как есть'
ACCESS_LOG=false # комментарий в конце строки
EMPTY=
";
    let pairs = parse_dotenv(text).expect(".env без ошибок разбирается");
    let get = |key: &str| {
        pairs
            .iter()
            .find(|(own, _)| own == key)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(get("USERS_GRPC_ADDR"), Some("127.0.0.1:50052"));
    assert_eq!(
        get("CERTS_DIR"),
        Some("certs"),
        "export и пробелы у = не мешают"
    );
    assert_eq!(get("GREETING"), Some("привет # это не комментарий"));
    assert_eq!(get("SINGLE"), Some("как есть"));
    assert_eq!(
        get("ACCESS_LOG"),
        Some("false"),
        "хвостовой комментарий отрезан"
    );
    assert_eq!(get("EMPTY"), Some(""));
}

#[test]
fn dotenv_names_every_broken_line_without_echoing_it() {
    let text = "OK=1\nэто просто текст\n=пусто\nPASSWORD hunter2\nALSO_OK=2\n";
    let problems = parse_dotenv(text).expect_err("кривые строки не проходят молча");
    assert_eq!(
        problems,
        vec![
            Problem::Dotenv { line: 2 },
            Problem::Dotenv { line: 3 },
            Problem::Dotenv { line: 4 },
        ],
        "все плохие строки разом, по номерам"
    );
    let shown: Vec<String> = problems.iter().map(ToString::to_string).collect();
    assert!(
        shown.iter().all(|line| !line.contains("hunter2")),
        "в сообщении номер строки, а не её текст: там может быть пароль: {shown:?}"
    );
}

#[test]
fn process_environment_wins_over_the_file() {
    let dir = scratch("dotenv-layers");
    let file = dir.join(".env");
    fs::write(&file, "PATH=из-файла\nM34_ONLY_IN_FILE=файл\n").expect("файл пишется");
    let source = Source::load(&file).expect(".env читается");
    assert_eq!(source.get("M34_ONLY_IN_FILE"), Some("файл"));
    assert_ne!(
        source.get("PATH"),
        Some("из-файла"),
        "переменная окружения процесса главнее .env: так её можно переопределить при запуске"
    );
    let missing = Source::load(dir.join("нет-такого.env")).expect("без .env тоже запускаемся");
    assert!(missing.get("M34_ONLY_IN_FILE").is_none());
    let unreadable = Source::load(&dir).expect_err("папка вместо файла это ошибка");
    assert!(matches!(
        unreadable.problems.as_slice(),
        [Problem::Unreadable { .. }]
    ));
    let layered = Source::from_pairs([("A", "файл"), ("B", "файл")])
        .over(Source::new().with("A", "окружение"));
    assert_eq!(layered.get("A"), Some("окружение"));
    assert_eq!(layered.get("B"), Some("файл"));
}

#[test]
fn fields_report_every_mistake_at_once() {
    let source = Source::from_pairs([
        ("ADDR", "localhost"),
        ("WAIT_MS", "скоро"),
        ("ACCESS_LOG", "наверное"),
        ("LIMIT", "-5"),
        ("BLANK", "   "),
        ("NAMES", "a, b,,c "),
    ]);
    let mut fields = source.fields();
    let addr = fields.addr("ADDR", "127.0.0.1:1");
    let wait = fields.millis("WAIT_MS", Duration::from_secs(10));
    let log = fields.flag("ACCESS_LOG", true);
    let limit: usize = fields.parse("LIMIT", 8);
    let blank: u32 = fields.parse("BLANK", 7);
    let token: Option<String> = fields.required("TOKEN");
    let names = fields.list("NAMES");
    assert_eq!(
        addr, "127.0.0.1:1",
        "после ошибки берётся умолчание, чтобы читать дальше"
    );
    assert_eq!(wait, Duration::from_secs(10));
    assert!(log);
    assert_eq!(limit, 8);
    assert_eq!(blank, 7, "пустое значение это «не задано», а не ошибка");
    assert_eq!(token, None);
    assert_eq!(names, ["a", "b", "c"]);
    let error = fields.finish().expect_err("пять ошибок не проходят");
    assert_eq!(
        error.keys(),
        ["ADDR", "WAIT_MS", "ACCESS_LOG", "LIMIT", "TOKEN"],
        "конфиг называет все плохие переменные сразу, а не по одной за запуск"
    );
    let text = error.to_string();
    for key in ["ADDR", "WAIT_MS", "ACCESS_LOG", "LIMIT", "TOKEN"] {
        assert!(text.contains(key), "{key} есть в сообщении: {text}");
    }
    assert!(
        text.contains("\"скоро\""),
        "сообщение показывает, что пришло: {text}"
    );
}

#[test]
fn good_values_pass_and_two_owners_of_one_port_do_not() {
    let source = Source::from_pairs([
        ("GRPC", "127.0.0.1:50052"),
        ("ADMIN", "127.0.0.1:50052"),
        ("OTHER", "127.0.0.1:0"),
        ("ANY", "127.0.0.1:0"),
        ("WAIT_MS", "250"),
    ]);
    let mut fields = source.fields();
    let grpc = fields.addr("GRPC", "x");
    let admin = fields.addr("ADMIN", "x");
    let other = fields.addr("OTHER", "x");
    let any = fields.addr("ANY", "x");
    assert_eq!(
        fields.millis("WAIT_MS", Duration::ZERO),
        Duration::from_millis(250)
    );
    fields.distinct_addrs(&[
        ("GRPC", &grpc),
        ("ADMIN", &admin),
        ("OTHER", &other),
        ("ANY", &any),
    ]);
    let error = fields.finish().expect_err("один порт на двоих");
    assert_eq!(
        error.keys(),
        ["ADMIN"],
        "порт 0 выбирает система, он не конфликтует"
    );
    assert!(error.to_string().contains("GRPC"), "{error}");
}

#[derive(Debug)]
struct Settings {
    user: String,
    password: Secret<String>,
}

#[test]
fn secrets_never_reach_logs() {
    let source = Source::new().with("DB_PASSWORD", "hunter2");
    let mut fields = source.fields();
    let password = fields.secret("DB_PASSWORD").expect("пароль задан");
    let missing = fields.secret("API_KEY");
    fields.reject_secret("DB_PASSWORD", "короче 12 символов");
    let error = fields.finish().expect_err("слабый пароль и нет ключа");
    assert_eq!(password.expose(), "hunter2", "внутри значение настоящее");
    assert!(missing.is_none());
    let settings = Settings {
        user: "app".to_string(),
        password,
    };
    let debug = format!("{settings:?}");
    assert!(!debug.contains("hunter2"), "{debug}");
    assert!(debug.contains("Secret(***)"), "{debug}");
    assert_eq!(settings.password.to_string(), "***");
    assert_eq!(settings.user, "app", "обычные поля печатаются как есть");
    let text = error.to_string();
    assert!(
        !text.contains("hunter2"),
        "и в ошибке конфига его нет: {text}"
    );
    assert!(text.contains("API_KEY: не задана"), "{text}");
}

#[tokio::test(start_paused = true)]
async fn checks_run_together_and_a_hung_one_is_a_timeout() {
    let health = Health::new(CHECK_LIMIT);
    for name in ["postgres", "nats", "users"] {
        health.register(name, || async {
            sleep(Duration::from_millis(400)).await;
            Ok(())
        });
    }
    health.register("cache", || async {
        sleep(Duration::from_secs(3600)).await;
        Ok(())
    });
    health.register("search", || async {
        Err("индекс не открылся".to_string())
    });
    let started = Instant::now();
    let report = health.report().await;
    assert_eq!(
        started.elapsed(),
        CHECK_LIMIT,
        "проверки идут одновременно: три по 400 мс это 400 мс, а зависшая обрезана лимитом"
    );
    assert!(!report.ready);
    let names: Vec<&str> = report.probes.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        ["postgres", "nats", "users", "cache", "search"],
        "порядок регистрации"
    );
    let failing = report.failing();
    assert_eq!(failing.len(), 2, "{failing:?}");
    assert_eq!(failing[0].0, "cache");
    assert!(failing[0].1.starts_with("timeout"), "{failing:?}");
    assert_eq!(failing[1], ("search", "индекс не открылся"));
}

#[tokio::test]
async fn stopping_answers_not_ready_without_asking_anyone() {
    let health = Health::default();
    let asked = Arc::new(AtomicU32::new(0));
    let counter = Arc::clone(&asked);
    health.register("db", move || {
        counter.fetch_add(1, Ordering::SeqCst);
        async { Ok(()) }
    });
    assert!(health.report().await.ready);
    health.stop();
    let report = health.report().await;
    assert!(!report.ready && report.stopping, "{report:?}");
    assert_eq!(
        asked.load(Ordering::SeqCst),
        1,
        "при остановке проверки не нужны"
    );
    assert_eq!(report.summary(), "идёт остановка");
}

fn buggy() -> Result<(), String> {
    panic!("ошибка в коде проверки")
}

#[tokio::test]
async fn a_panicking_check_fails_under_its_own_name() {
    let health = Health::default();
    health.register("ok", || async { Ok(()) });
    health.register("buggy", || async { buggy() });
    let report = health.report().await;
    assert!(!report.ready);
    assert_eq!(report.failing(), [("buggy", "проверка упала с паникой")]);
}

#[tokio::test(start_paused = true)]
async fn waiting_has_a_pause_and_one_deadline() {
    let policy = Policy {
        interval: Duration::from_millis(100),
        total: Duration::from_secs(1),
    };
    let started = Instant::now();
    let mut seen = 0;
    let gave_up = wait_for(&policy, || {
        seen += 1;
        let attempt = seen;
        async move { Err::<(), _>(format!("попытка {attempt}: отказ")) }
    })
    .await
    .expect_err("зависимость так и не ответила");
    assert_eq!(
        gave_up.attempts, 11,
        "0, 100, ... 1000 мс: попытки с паузой"
    );
    assert_eq!(
        started.elapsed(),
        Duration::from_secs(1),
        "и ни мигом дольше дедлайна"
    );
    assert_eq!(gave_up.last.as_deref(), Some("попытка 11: отказ"));
    let started = Instant::now();
    let mut left = 2;
    let attempts = wait_for(&policy, || {
        left -= 1;
        let ready = left < 0;
        async move { if ready { Ok(()) } else { Err("ещё нет") } }
    })
    .await
    .expect("на третьей попытке поднялась");
    assert_eq!(attempts, 3);
    assert_eq!(started.elapsed(), Duration::from_millis(200));
    let hung = wait_for(&policy, || async {
        sleep(Duration::from_secs(3600)).await;
        Ok::<(), String>(())
    })
    .await
    .expect_err("зависшая проверка не вешает старт");
    assert_eq!(hung.last, None);
}

#[tokio::test(start_paused = true)]
async fn health_wait_names_what_it_did_not_get() {
    let health = Health::default();
    health.register("journal", || async { Ok(()) });
    health.register("user-service", || async {
        Err("нет связи".to_string())
    });
    let policy = Policy {
        interval: Duration::from_millis(200),
        total: Duration::from_secs(2),
    };
    let error = health
        .wait(&policy)
        .await
        .expect_err("user-service не поднялся");
    assert_eq!(
        error.failing,
        [("user-service".to_string(), "нет связи".to_string())]
    );
    assert_eq!(error.attempts, 11);
    let text = error.to_string();
    assert!(
        text.contains("user-service (нет связи)") && text.contains("2000 мс"),
        "{text}"
    );
}

struct Alive(Arc<AtomicBool>);

impl Drop for Alive {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

const QUICK: Restart = Restart {
    pause: Duration::from_millis(10),
    max_restarts: 2,
};

#[tokio::test(start_paused = true)]
async fn an_optional_task_gives_up_and_the_service_lives_on() {
    let trail = Trail::quiet();
    let mut background = Background::new(trail.clone());
    background.spawn("thumbnails", Criticality::Optional, QUICK, || async {
        panic!("кривая картинка");
    });
    let token = CancellationToken::new();
    let run = tokio::spawn(background.run(token.clone()));
    sleep(Duration::from_secs(1)).await;
    assert!(
        !run.is_finished(),
        "некритичная задача не роняет остальной фон"
    );
    assert!(
        trail
            .lines()
            .iter()
            .any(|line| line.contains("thumbnails") && line.contains("3 раз")),
        "о выключенной задаче сказано вслух: {:?}",
        trail.lines()
    );
    token.cancel();
    assert_eq!(run.await.expect("надзиратель не паникует"), Ok(()));
}

#[tokio::test(start_paused = true)]
async fn a_critical_task_that_keeps_failing_is_fatal() {
    let alive = Arc::new(AtomicBool::new(true));
    let mut background = Background::new(Trail::quiet());
    let flag = Arc::clone(&alive);
    background.spawn("feed-sync", Criticality::Optional, QUICK, move || {
        let guard = Alive(Arc::clone(&flag));
        async move {
            let _guard = guard;
            sleep(Duration::from_secs(3600)).await;
        }
    });
    background.spawn("indexer", Criticality::Critical, QUICK, || async {
        panic!("индекс испорчен");
    });
    let fatal = background
        .run(CancellationToken::new())
        .await
        .expect_err("паника критичной задачи не проходит молча");
    assert_eq!(fatal.name, "indexer");
    assert_eq!(fatal.attempts, 3, "первый запуск и два перезапуска");
    assert!(
        !alive.load(Ordering::SeqCst),
        "остальные задачи остановлены, а не брошены работать"
    );
}

#[tokio::test(start_paused = true)]
async fn cancelling_the_supervisor_cancels_its_tasks() {
    let alive = Arc::new(AtomicBool::new(true));
    let flag = Arc::clone(&alive);
    let mut background = Background::new(Trail::quiet());
    background.spawn("cleanup", Criticality::Critical, QUICK, move || {
        let guard = Alive(Arc::clone(&flag));
        async move {
            let _guard = guard;
            loop {
                sleep(Duration::from_secs(1)).await;
            }
        }
    });
    let token = CancellationToken::new();
    let run = tokio::spawn(background.run(token.clone()));
    sleep(Duration::from_secs(5)).await;
    assert!(alive.load(Ordering::SeqCst));
    token.cancel();
    assert_eq!(run.await.expect("надзиратель"), Ok(()));
    assert!(
        !alive.load(Ordering::SeqCst),
        "без сирот: задача умерла вместе с надзирателем"
    );
}

type Events = Arc<Mutex<Vec<String>>>;

fn note(events: &Events, what: &str) {
    events
        .lock()
        .expect("журнал событий")
        .push(what.to_string());
}

fn timing(grace: u64, drain: u64) -> Timing {
    Timing {
        grace: Duration::from_millis(grace),
        drain: Duration::from_millis(drain),
        wait: None,
    }
}

#[tokio::test(start_paused = true)]
async fn stopping_goes_in_order() {
    let signal = CancellationToken::new();
    let lifecycle =
        Lifecycle::new(&signal, Health::default(), Trail::quiet()).timing(timing(1000, 2000));
    let events: Events = Arc::default();
    let closing = lifecycle.closing();
    let seen = Arc::clone(&events);
    let serving = async move {
        closing.cancelled().await;
        note(&seen, "приём закрыт");
        sleep(Duration::from_millis(300)).await;
        note(&seen, "последний запрос ответил");
        Ok::<(), String>(())
    };
    let mut background = Background::new(Trail::quiet());
    let seen = Arc::clone(&events);
    background.spawn("outbox", Criticality::Critical, QUICK, move || {
        let seen = Arc::clone(&seen);
        async move {
            struct Stopped(Events);
            impl Drop for Stopped {
                fn drop(&mut self) {
                    note(&self.0, "фон остановлен");
                }
            }
            let _stopped = Stopped(seen);
            std::future::pending::<()>().await;
        }
    });
    let watcher = lifecycle.clone();
    let seen = Arc::clone(&events);
    let probe = tokio::spawn(async move {
        sleep(Duration::from_millis(100)).await;
        let stop_at = Instant::now();
        signal.cancel();
        sleep(Duration::from_millis(500)).await;
        let report = watcher.health().report().await;
        note(&seen, &format!("через 500 мс ready={}", report.ready));
        assert!(
            !watcher.closing().is_cancelled(),
            "в паузе grace приём ещё открыт"
        );
        stop_at
    });
    let started = Instant::now();
    lifecycle
        .run(None, serving, background)
        .await
        .expect("чистая остановка");
    let stop_at = probe.await.expect("наблюдатель");
    assert_eq!(
        events.lock().expect("журнал").clone(),
        [
            "через 500 мс ready=false",
            "приём закрыт",
            "последний запрос ответил",
            "фон остановлен",
        ],
        "сначала не готов, потом закрыть приём, дождаться запросов и только потом гасить фон"
    );
    assert_eq!(
        lifecycle.steps(),
        [
            Step::NotReady,
            Step::Closed,
            Step::Drained,
            Step::BackgroundStopped,
            Step::Stopped,
        ]
    );
    assert_eq!(stop_at - started, Duration::from_millis(100));
    assert_eq!(
        started.elapsed(),
        Duration::from_millis(100 + 1000 + 300),
        "grace, потом ровно столько, сколько шли запросы"
    );
    assert!(
        lifecycle.finished().is_cancelled(),
        "admin-порт гаснет последним"
    );
}

#[tokio::test(start_paused = true)]
async fn a_hung_request_is_cut_at_the_drain_deadline() {
    let signal = CancellationToken::new();
    let lifecycle =
        Lifecycle::new(&signal, Health::default(), Trail::quiet()).timing(timing(100, 2000));
    let stop = signal.clone();
    tokio::spawn(async move {
        sleep(Duration::from_millis(10)).await;
        stop.cancel();
    });
    let serving = async {
        std::future::pending::<()>().await;
        Ok::<(), String>(())
    };
    let started = Instant::now();
    lifecycle
        .run(None, serving, Background::new(Trail::quiet()))
        .await
        .expect("обрыв по дедлайну это не ошибка процесса");
    assert_eq!(started.elapsed(), Duration::from_millis(10 + 100 + 2000));
    assert!(
        lifecycle.steps().contains(&Step::Abandoned),
        "{:?}",
        lifecycle.steps()
    );
}

#[tokio::test(start_paused = true)]
async fn a_dead_critical_task_stops_the_whole_service_in_order() {
    let signal = CancellationToken::new();
    let trail = Trail::quiet();
    let lifecycle =
        Lifecycle::new(&signal, Health::default(), trail.clone()).timing(timing(100, 500));
    let closing = lifecycle.closing();
    let serving = async move {
        closing.cancelled().await;
        Ok::<(), String>(())
    };
    let mut background = Background::new(trail.clone());
    background.spawn("indexer", Criticality::Critical, QUICK, || async {
        panic!("индекс испорчен");
    });
    let halted = lifecycle.run(None, serving, background).await;
    match halted {
        Err(Halt::Fatal(fatal)) => assert_eq!(fatal.name, "indexer"),
        other => panic!("ждали Fatal, а пришло {other:?}"),
    }
    assert!(
        lifecycle.stopping().is_cancelled(),
        "остальные части сервиса узнали об остановке"
    );
    assert!(
        !signal.is_cancelled(),
        "чужой токен (сигнал процесса) не трогаем"
    );
    assert_eq!(lifecycle.steps().first(), Some(&Step::NotReady));
    assert_eq!(lifecycle.steps().last(), Some(&Step::Stopped));
    assert!(
        trail
            .lines()
            .iter()
            .any(|line| line.contains("приём закрыт")),
        "шаги остановки видны в журнале: {:?}",
        trail.lines()
    );
}

#[tokio::test(start_paused = true)]
async fn startup_gives_up_and_never_serves() {
    let signal = CancellationToken::new();
    let health = Health::default();
    health.register("user-service", || async {
        Err("нет связи".to_string())
    });
    let lifecycle = Lifecycle::new(&signal, health, Trail::quiet());
    let served = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&served);
    let serving = async move {
        flag.store(true, Ordering::SeqCst);
        Ok::<(), String>(())
    };
    let policy = Policy {
        interval: Duration::from_millis(100),
        total: Duration::from_millis(500),
    };
    let started = Instant::now();
    let halted = lifecycle
        .run(Some(policy), serving, Background::new(Trail::quiet()))
        .await;
    match halted {
        Err(Halt::Startup(error)) => {
            assert_eq!(error.failing[0].0, "user-service");
            assert_eq!(error.attempts, 6);
        }
        other => panic!("ждали Startup, а пришло {other:?}"),
    }
    assert_eq!(
        started.elapsed(),
        Duration::from_millis(500),
        "без grace: трафика ещё не было"
    );
    assert!(
        !served.load(Ordering::SeqCst),
        "не дождался зависимостей: не принимает запросы"
    );
}

#[tokio::test(start_paused = true)]
async fn a_signal_during_startup_stops_without_serving() {
    let signal = CancellationToken::new();
    let health = Health::default();
    health.register("user-service", || async {
        Err("нет связи".to_string())
    });
    let lifecycle = Lifecycle::new(&signal, health, Trail::quiet());
    let stop = signal.clone();
    tokio::spawn(async move {
        sleep(Duration::from_millis(300)).await;
        stop.cancel();
    });
    let started = Instant::now();
    lifecycle
        .run(
            Some(Policy::default()),
            async { Ok::<(), String>(()) },
            Background::new(Trail::quiet()),
        )
        .await
        .expect("SIGTERM во время ожидания это чистый выход");
    assert_eq!(started.elapsed(), Duration::from_millis(300));
}

#[cfg(unix)]
#[tokio::test]
async fn signals_arrive_as_values_instead_of_killing_the_process() {
    let mut signals = Signals::listen().expect("подписка на сигналы");
    let pid = std::process::id().to_string();
    for (flag, expected) in [("-TERM", Signal::Terminate), ("-INT", Signal::Interrupt)] {
        let sent = std::process::Command::new("kill")
            .args([flag, &pid])
            .status()
            .expect("kill запускается");
        assert!(sent.success());
        let got = timeout(Duration::from_secs(5), signals.next())
            .await
            .expect("сигнал дошёл");
        assert_eq!(got, expected);
    }
}
