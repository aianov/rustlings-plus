use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use devctl::{Devctl, Plan, Stopped, Unit, alive, social};
use service_runtime::Source;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn state(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("m50-devctl")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

async fn ready_page() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = listener.local_addr().expect("адрес");
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 512];
                let _ = socket.read(&mut buf).await;
                let _ = socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                    .await;
            });
        }
    });
    addr
}

fn sleeper(name: &str, service: &str, ready: SocketAddr) -> Unit {
    Unit::new(name, service, "sh")
        .arg("-c")
        .arg("exec sleep 30")
        .ready(&format!("http://{ready}/ready"))
}

#[test]
fn units_start_in_dependency_order() {
    let plan = Plan::default()
        .with(
            Unit::new("gw", "gateway", "x")
                .after("posts")
                .after("users"),
        )
        .with(Unit::new("posts-1", "posts", "x").after("users"))
        .with(Unit::new("posts-2", "posts", "x").after("users"))
        .with(Unit::new("users", "users", "x"));
    let order: Vec<&str> = plan
        .order()
        .expect("порядок есть")
        .iter()
        .map(|unit| unit.name.as_str())
        .collect();
    assert_eq!(order, ["users", "posts-1", "posts-2", "gw"]);
    let circle = Plan::default()
        .with(Unit::new("a", "a", "x").after("b"))
        .with(Unit::new("b", "b", "x").after("a"));
    assert!(circle.order().expect_err("круг").contains("по кругу"));
    let lost = Plan::default().with(Unit::new("a", "a", "x").after("nobody"));
    assert!(lost.order().is_err());
    let social = social(&Source::new(), std::path::Path::new("target/debug"));
    let names: Vec<&str> = social
        .order()
        .expect("план соцсети")
        .iter()
        .map(|u| u.name.as_str())
        .collect();
    assert_eq!(names.first(), Some(&"user-service"));
    assert_eq!(names.last(), Some(&"api-gateway-2"));
    let replicas = social
        .units
        .iter()
        .find(|unit| unit.name == "api-gateway-1")
        .and_then(|unit| {
            unit.env
                .iter()
                .find(|(key, _)| key == "GATEWAY_POSTS_REPLICAS")
        })
        .map(|(_, value)| value.clone());
    assert_eq!(replicas.as_deref(), Some("127.0.0.1:50053,127.0.0.1:50063"));
}

#[tokio::test]
async fn up_and_down_leave_no_orphans() {
    let ready = ready_page().await;
    let plan = Plan::default()
        .with(sleeper("a", "a", ready))
        .with(sleeper("b", "b", ready).after("a"));
    let devctl = Devctl::new(state("updown"));
    assert_eq!(devctl.up(&plan).await.expect("подъём"), ["a", "b"]);
    let pids: Vec<u32> = ["a", "b"]
        .iter()
        .map(|name| devctl.pid(name).expect("pid-файл"))
        .collect();
    for pid in &pids {
        assert!(alive(*pid).await);
    }
    let status = devctl.status(&plan).await;
    assert!(
        status
            .iter()
            .all(|instance| instance.alive && instance.ready),
        "{status:?}"
    );
    let stopped = devctl.down(&plan).await.expect("остановка");
    assert_eq!(
        stopped,
        [
            (String::from("b"), Stopped::Terminated),
            (String::from("a"), Stopped::Terminated)
        ],
        "останавливаются в обратном порядке"
    );
    assert!(
        devctl.pid("a").is_none() && devctl.pid("b").is_none(),
        "pid-файлы убраны"
    );
}

#[tokio::test]
async fn a_stubborn_process_is_killed_after_the_deadline() {
    let ready = ready_page().await;
    let stubborn = Unit::new("stubborn", "stubborn", "sh")
        .arg("-c")
        .arg("trap '' TERM; while true; do sleep 1; done")
        .ready(&format!("http://{ready}/ready"));
    let plan = Plan::default().with(stubborn);
    let devctl = Devctl::new(state("stubborn")).grace(Duration::from_millis(300));
    devctl.up(&plan).await.expect("подъём");
    let started = std::time::Instant::now();
    assert_eq!(devctl.stop("stubborn").await, Stopped::Killed);
    assert!(
        started.elapsed() >= Duration::from_millis(300),
        "сначала ждали SIGTERM"
    );
}

#[tokio::test]
async fn a_failing_unit_rolls_the_start_back() {
    let ready = ready_page().await;
    let plan = Plan::default().with(sleeper("good", "good", ready)).with(
        Unit::new("bad", "bad", "sh")
            .arg("-c")
            .arg("echo не могу; exit 3")
            .ready("http://127.0.0.1:1/ready")
            .after("good"),
    );
    let devctl = Devctl::new(state("rollback")).ready_within(Duration::from_secs(3));
    let error = devctl.up(&plan).await.expect_err("bad не поднимается");
    assert!(error.contains("bad"), "{error}");
    assert!(devctl.pid("good").is_none(), "поднятое до сбоя остановлено");
}

#[tokio::test]
async fn restart_rolls_instances_one_by_one() {
    let ready = ready_page().await;
    let plan = Plan::default()
        .with(sleeper("web-1", "web", ready))
        .with(sleeper("web-2", "web", ready));
    let devctl = Devctl::new(state("restart"));
    devctl.up(&plan).await.expect("подъём");
    let before = (devctl.pid("web-1"), devctl.pid("web-2"));
    let rolled = devctl.restart(&plan, "web").await.expect("перезапуск");
    assert_eq!(rolled, ["web-1", "web-2"], "по одному в порядке плана");
    let after = (devctl.pid("web-1"), devctl.pid("web-2"));
    assert_ne!(before.0, after.0);
    assert_ne!(before.1, after.1);
    assert!(
        devctl
            .status(&plan)
            .await
            .iter()
            .all(|instance| instance.alive)
    );
    assert!(devctl.restart(&plan, "nobody").await.is_err());
    devctl.down(&plan).await.expect("остановка");
}
