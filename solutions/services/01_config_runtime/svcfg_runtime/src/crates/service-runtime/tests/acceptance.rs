use futures::future::BoxFuture;
use service_runtime::config::{ConfigError, Secret, layered, parse_dotenv, parse_or, required};
use service_runtime::health::{Check, Health};
use service_runtime::lifecycle::{Lifecycle, begin_request};
use service_runtime::startup::{Policy, StartupError, wait_for};
use service_runtime::supervisor::{RestartPolicy, SupervisorError, TaskSpec, run};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::watch;
use tokio::time::{Instant, sleep, timeout};

#[test]
fn config_helpers_work_together() {
    let file = parse_dotenv("# локально\nPORT=9000\nTOKEN=\"a b # c\"\nURL=postgres://h/db?x=1\n").unwrap();
    assert_eq!(file["TOKEN"], "a b # c", "кавычки защищают пробелы и решётку");
    assert_eq!(file["URL"], "postgres://h/db?x=1", "`=` внутри значения сохраняется");
    let source = layered(&file, |name| (name == "PORT").then(|| "1".to_string()));
    assert_eq!(source("PORT").as_deref(), Some("1"), "окружение важнее файла");
    assert_eq!(source("TOKEN").as_deref(), Some("a b # c"), "остальное из файла");
    assert_eq!(parse_dotenv("A=1\nмусор").unwrap_err().line, 2, "номер строки с ошибкой");
}

#[test]
fn typed_parsing_names_the_variable() {
    assert_eq!(parse_or("PORT", None, 8080u16), Ok(8080), "умолчание");
    assert_eq!(parse_or("PORT", Some(" 81 ".to_string()), 8080u16), Ok(81), "значение с пробелами");
    let err = parse_or("PORT", Some("abc".to_string()), 8080u16).unwrap_err();
    assert_eq!(err, ConfigError::Invalid { name: "PORT".to_string(), value: "abc".to_string() }, "имя и значение");
    assert_eq!(required("DB", None), Err(ConfigError::Missing("DB".to_string())), "обязательная переменная");
    assert_eq!(required("DB", Some("  ".to_string())), Err(ConfigError::Missing("DB".to_string())), "пустое значение считается отсутствующим");
    assert!(err.to_string().contains("PORT"), "Display называет переменную");
}

#[test]
fn secrets_do_not_leak() {
    let secret = Secret::new("hunter2".to_string());
    assert_eq!(format!("{secret:?} {secret}"), "Secret(***) ***", "Debug и Display");
    assert_eq!(secret.expose(), "hunter2", "expose отдаёт значение");
}

struct Flag {
    name: &'static str,
    ok: Arc<AtomicBool>,
}

impl Check for Flag {
    fn name(&self) -> &str {
        self.name
    }

    fn run(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            if self.ok.load(Ordering::SeqCst) { Ok(()) } else { Err("down".to_string()) }
        })
    }
}

#[tokio::test(start_paused = true)]
async fn ready_follows_dependencies_but_live_does_not() {
    let db = Arc::new(AtomicBool::new(true));
    let mut health = Health::new(Duration::from_secs(1));
    health.register(Arc::new(Flag { name: "db", ok: Arc::clone(&db) }));
    assert!(health.ready().await.ready, "база отвечает: готов");
    db.store(false, Ordering::SeqCst);
    let report = health.ready().await;
    assert!(!report.ready && report.failures[0].0 == "db", "база упала: не готов, причина называет db");
    assert!(health.live(), "liveness от базы не зависит");
    db.store(true, Ordering::SeqCst);
    health.begin_shutdown();
    assert!(!health.ready().await.ready, "при остановке не готов");
    assert!(health.live(), "но жив");
}

#[tokio::test(start_paused = true)]
async fn startup_waits_for_dependencies_and_gives_up() {
    let policy = Policy { interval: Duration::from_millis(100), total: Duration::from_millis(350) };
    let calls = AtomicU32::new(0);
    let attempts = wait_for("db", &policy, || async {
        if calls.fetch_add(1, Ordering::SeqCst) < 2 { Err("no".to_string()) } else { Ok(()) }
    })
    .await;
    assert_eq!(attempts, Ok(3), "две неудачи и успех: три попытки");
    let started = Instant::now();
    let err = wait_for("db", &policy, || async { Err("no".to_string()) }).await.unwrap_err();
    assert!(matches!(err, StartupError::Dependency { attempts: 4, .. }), "четыре попытки до дедлайна: {err:?}");
    assert!(started.elapsed() <= policy.total, "общий дедлайн соблюдён");
}

#[tokio::test(start_paused = true)]
async fn shutdown_follows_the_order() {
    let ticks = Arc::new(AtomicUsize::new(0));
    let background = {
        let ticks = Arc::clone(&ticks);
        tokio::spawn(async move {
            loop {
                ticks.fetch_add(1, Ordering::SeqCst);
                sleep(Duration::from_millis(10)).await;
            }
        })
    };
    let lifecycle = Lifecycle::new(vec![background]);
    let state = lifecycle.state();
    let guard = begin_request(&state).expect("запросы принимаются");
    tokio::spawn(async move {
        sleep(Duration::from_millis(300)).await;
        drop(guard);
    });
    let task = tokio::spawn(lifecycle.shutdown(Duration::from_millis(100), Duration::from_secs(5)));
    sleep(Duration::from_millis(50)).await;
    assert!(!state.ready.load(Ordering::SeqCst), "готовность снята сразу");
    assert!(state.accepting.load(Ordering::SeqCst), "в паузу grace запросы ещё принимаются");
    sleep(Duration::from_millis(100)).await;
    assert!(begin_request(&state).is_none(), "после grace новые запросы не принимаются");
    let before = ticks.load(Ordering::SeqCst);
    sleep(Duration::from_millis(100)).await;
    assert!(ticks.load(Ordering::SeqCst) > before, "фон работает, пока дренируются запросы");
    let outcome = task.await.unwrap();
    assert!(outcome.drained, "запрос дождались");
    let frozen = ticks.load(Ordering::SeqCst);
    sleep(Duration::from_millis(100)).await;
    assert_eq!(ticks.load(Ordering::SeqCst), frozen, "после остановки фон молчит");
}

#[tokio::test(start_paused = true)]
async fn critical_task_failure_ends_the_run() {
    let ticks = Arc::new(AtomicUsize::new(0));
    let ticker = {
        let ticks = Arc::clone(&ticks);
        TaskSpec {
            name: "ticker",
            critical: false,
            factory: Box::new(move || {
                let ticks = Arc::clone(&ticks);
                Box::pin(async move {
                    loop {
                        ticks.fetch_add(1, Ordering::SeqCst);
                        sleep(Duration::from_millis(10)).await;
                    }
                })
            }),
        }
    };
    let broken = TaskSpec {
        name: "indexer",
        critical: true,
        factory: Box::new(|| Box::pin(async { panic!("сломался") })),
    };
    let (_tx, rx) = watch::channel(false);
    let policy = RestartPolicy { max_restarts: 2, pause: Duration::from_millis(50) };
    let result = timeout(Duration::from_secs(10), run(vec![ticker, broken], policy, rx))
        .await
        .expect("критичный отказ должен завершать run");
    assert_eq!(
        result,
        Err(SupervisorError::CriticalFailed { name: "indexer", last_error: "паника".to_string() }),
        "паника считается сбоем с описанием «паника»"
    );
    let after = ticks.load(Ordering::SeqCst);
    sleep(Duration::from_millis(100)).await;
    assert_eq!(ticks.load(Ordering::SeqCst), after, "остальные задачи остановлены");
}

fn rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(rs_files(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out
}

#[test]
fn runtime_does_not_read_the_environment_itself() {
    for path in rs_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src")) {
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            !text.contains("env::var") && !text.contains("set_var"),
            "{} читает окружение напрямую: библиотека принимает источник значений снаружи",
            path.display()
        );
    }
}
