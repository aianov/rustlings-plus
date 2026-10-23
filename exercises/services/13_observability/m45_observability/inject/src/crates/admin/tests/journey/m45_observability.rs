use std::time::Duration;

use admin::{METRICS_TYPE, serve};
use service_runtime::{CancellationToken, Health, metrics};
use social_cli::Web;
use tokio::net::TcpListener;
use tokio::time::timeout;

#[tokio::test]
async fn metrics_live_on_the_admin_port() {
    metrics().inc(
        "m45_admin_checks_total",
        "проверка admin-порта",
        &[("kind", "test")],
        2,
    );
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("порт 0 свободен");
    let addr = listener.local_addr().expect("адрес");
    let stop = CancellationToken::new();
    let task = tokio::spawn(serve(
        listener,
        Health::new(Duration::from_secs(1)),
        stop.clone(),
    ));
    let reply = timeout(
        Duration::from_secs(5),
        Web::new(&addr.to_string()).request("GET", "/metrics", None, None),
    )
    .await
    .expect("admin-порт отвечает быстро")
    .expect("admin-порт отвечает");
    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("content-type"), Some(METRICS_TYPE));
    assert!(
        reply.body.contains(
            "# TYPE m45_admin_checks_total counter\nm45_admin_checks_total{kind=\"test\"} 2"
        ),
        "{}",
        reply.body
    );
    stop.cancel();
    let _ = timeout(Duration::from_secs(5), task).await;
}
