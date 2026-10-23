use ntx_health::{serve, status_of, wait_serving, Observed, ServingStatus, SERVICE};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tonic::transport::{Channel, Endpoint};
use tonic::Code;

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

fn channel(addr: std::net::SocketAddr) -> Channel {
    Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_lazy()
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[tokio::test]
async fn reported_status_is_what_the_client_sees() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ch = channel(listener.local_addr().unwrap());
    let (reporter, _server) = serve(listener, ServingStatus::NotServing).await;
    assert_eq!(within("check", status_of(&ch, SERVICE)).await.unwrap(), Observed::NotServing);
    reporter.set_service_status(SERVICE, ServingStatus::Serving).await;
    assert_eq!(within("check", status_of(&ch, SERVICE)).await.unwrap(), Observed::Serving);
}

#[tokio::test]
async fn unknown_service_name_is_not_found() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ch = channel(listener.local_addr().unwrap());
    let (_reporter, _server) = serve(listener, ServingStatus::Serving).await;
    let err = within("check", status_of(&ch, "no.such.Service")).await.unwrap_err();
    assert_eq!(err.code(), Code::NotFound, "неизвестное имя сервиса: по протоколу NotFound");
}

#[tokio::test]
async fn ready_service_is_awaited_instantly() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ch = channel(listener.local_addr().unwrap());
    let (_reporter, _server) = serve(listener, ServingStatus::Serving).await;
    let started = Instant::now();
    within("ожидание", wait_serving(&ch, SERVICE, Duration::from_secs(3))).await.unwrap();
    assert!(started.elapsed() < ms(400), "готовый сервис ждали {:?}", started.elapsed());
}

#[tokio::test]
async fn waits_until_the_status_flips() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ch = channel(listener.local_addr().unwrap());
    let (reporter, _server) = serve(listener, ServingStatus::NotServing).await;
    tokio::spawn(async move {
        tokio::time::sleep(ms(250)).await;
        reporter.set_service_status(SERVICE, ServingStatus::Serving).await;
    });
    let started = Instant::now();
    within("ожидание", wait_serving(&ch, SERVICE, Duration::from_secs(3))).await.unwrap();
    assert!(started.elapsed() >= ms(200), "вернулись раньше, чем сервис стал готов: {:?}", started.elapsed());
    assert!(started.elapsed() < ms(900), "ожидание затянулось: {:?}", started.elapsed());
}

#[tokio::test]
async fn gives_up_with_deadline_exceeded() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ch = channel(listener.local_addr().unwrap());
    let (_reporter, _server) = serve(listener, ServingStatus::NotServing).await;
    let started = Instant::now();
    let err = within("ожидание", wait_serving(&ch, SERVICE, ms(400))).await.unwrap_err();
    assert_eq!(err.code(), Code::DeadlineExceeded);
    assert!(started.elapsed() >= ms(350) && started.elapsed() < ms(1000), "срок ожидания 400 мс, а ждали {:?}", started.elapsed());
}

#[tokio::test]
async fn neighbour_that_starts_late_is_still_awaited() {
    let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);
    let ch = channel(addr);
    let waiter = tokio::spawn({
        let ch = ch.clone();
        async move { wait_serving(&ch, SERVICE, Duration::from_secs(4)).await }
    });
    tokio::time::sleep(ms(300)).await;
    let listener = TcpListener::bind(addr).await.unwrap();
    let (_reporter, _server) = serve(listener, ServingStatus::Serving).await;
    within("ожидание", waiter).await.unwrap().expect("сосед поднялся позже, а ожидание сдалось: ошибки подключения надо переживать, а не пробрасывать");
}
