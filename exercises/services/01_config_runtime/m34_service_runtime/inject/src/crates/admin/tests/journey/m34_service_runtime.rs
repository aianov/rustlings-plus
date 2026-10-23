use std::net::SocketAddr;
use std::time::Duration;

use admin::serve;
use contracts::{LiveView, ReadyView, decode};
use service_runtime::{CancellationToken, Health};
use social_cli::{Reply, Web};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep, timeout};

struct Port {
    addr: SocketAddr,
    stop: CancellationToken,
    task: JoinHandle<std::io::Result<()>>,
}

async fn start(health: Health) -> Port {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("порт 0 свободен");
    let addr = listener.local_addr().expect("адрес");
    let stop = CancellationToken::new();
    let task = tokio::spawn(serve(listener, health, stop.clone()));
    Port { addr, stop, task }
}

async fn get(addr: SocketAddr, path: &str) -> Reply {
    timeout(
        Duration::from_secs(5),
        Web::new(&addr.to_string()).request("GET", path, None, None),
    )
    .await
    .expect("admin-порт отвечает быстро")
    .expect("admin-порт отвечает")
}

#[tokio::test]
async fn live_answers_at_once_even_when_a_check_hangs() {
    let health = Health::new(Duration::from_secs(3));
    health.register("broker", || async {
        sleep(Duration::from_secs(3600)).await;
        Ok(())
    });
    let port = start(health).await;
    let started = Instant::now();
    let live = get(port.addr, "/health").await;
    assert_eq!(live.status, 200, "{live:?}");
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "/health не ходит по зависимостям: иначе зависший брокер убьёт все экземпляры"
    );
    let view: LiveView = decode(&live.body).expect("тело /health это JSON");
    assert_eq!(view.status, "live");
    port.stop.cancel();
    port.task
        .await
        .expect("сервер")
        .expect("останавливается чисто");
}

#[tokio::test]
async fn ready_names_every_failing_check_with_a_reason() {
    let health = Health::default();
    health.register("journal", || async { Ok(()) });
    health.register("user-service", || async {
        Err("нет связи".to_string())
    });
    let port = start(health).await;
    let ready = get(port.addr, "/ready").await;
    assert_eq!(ready.status, 503, "{ready:?}");
    assert_eq!(
        ready.header("content-type"),
        Some("application/json"),
        "{ready:?}"
    );
    let view: ReadyView = decode(&ready.body).expect("тело /ready это JSON");
    assert!(!view.ready && !view.stopping);
    let names: Vec<&str> = view
        .checks
        .iter()
        .map(|check| check.name.as_str())
        .collect();
    assert_eq!(names, ["journal", "user-service"]);
    assert!(view.checks[0].ok && view.checks[0].reason.is_none());
    assert_eq!(view.checks[1].reason.as_deref(), Some("нет связи"));
    assert_eq!(view.summary(), "user-service (нет связи)");
    port.stop.cancel();
}

#[tokio::test]
async fn ready_is_200_when_everything_answers_and_503_while_stopping() {
    let health = Health::default();
    health.register("journal", || async { Ok(()) });
    let port = start(health.clone()).await;
    let ready = get(port.addr, "/ready").await;
    assert_eq!(ready.status, 200, "{ready:?}");
    let view: ReadyView = decode(&ready.body).expect("JSON");
    assert!(view.ready);
    assert_eq!(view.summary(), "готов");
    health.stop();
    let stopping = get(port.addr, "/ready").await;
    assert_eq!(stopping.status, 503, "первый шаг остановки: снять трафик");
    let view: ReadyView = decode(&stopping.body).expect("JSON");
    assert!(view.stopping && view.checks.is_empty(), "{view:?}");
    let live = get(port.addr, "/health").await;
    assert_eq!(
        live.status, 200,
        "во время остановки процесс жив, перезапускать его не надо"
    );
    port.stop.cancel();
}

#[tokio::test]
async fn unknown_paths_are_404_and_the_port_closes_with_its_token() {
    let port = start(Health::default()).await;
    assert_eq!(get(port.addr, "/metrics").await.status, 404);
    port.stop.cancel();
    timeout(Duration::from_secs(5), port.task)
        .await
        .expect("admin-порт закрывается по токену")
        .expect("задача")
        .expect("без ошибки");
    let refused = Web::new(&port.addr.to_string())
        .request("GET", "/health", None, None)
        .await;
    assert!(refused.is_err(), "порт закрыт: {refused:?}");
}
