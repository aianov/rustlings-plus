use ntx_shutdown::pb::work_client::WorkClient;
use ntx_shutdown::pb::RunRequest;
use ntx_shutdown::{run, SERVICE};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tonic::transport::{Channel, Endpoint};
use tonic::Code;
use tonic_health::pb::health_client::HealthClient;
use tonic_health::pb::HealthCheckRequest;
use tonic_health::pb::health_check_response::ServingStatus;

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

async fn health(client: &mut HealthClient<Channel>) -> Result<ServingStatus, tonic::Status> {
    Ok(client.check(HealthCheckRequest { service: SERVICE.to_string() }).await?.into_inner().status())
}

#[tokio::test]
async fn shutdown_goes_through_not_serving_and_grace_while_calls_finish() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, signal) = oneshot::channel();
    let started = Instant::now();
    let server = tokio::spawn(async move {
        let result = run(listener, signal, ms(300)).await;
        (result, started.elapsed())
    });
    let ch = Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_lazy();
    let mut hc = HealthClient::new(ch.clone());
    let mut work = WorkClient::new(ch.clone());
    let mut ready = false;
    for _ in 0..40 {
        if matches!(health(&mut hc).await, Ok(ServingStatus::Serving)) {
            ready = true;
            break;
        }
        tokio::time::sleep(ms(50)).await;
    }
    assert!(ready, "сервер не стал SERVING: в начале сервис work.v1.Work должен быть готов");
    let long_started = Instant::now();
    let mut long_client = work.clone();
    let long = tokio::spawn(async move { long_client.run(RunRequest { ms: 600 }).await.map(|_| long_started.elapsed()) });
    tokio::time::sleep(ms(100)).await;
    stop.send(()).unwrap();
    tokio::time::sleep(ms(50)).await;
    let during = within("здоровье во время грации", health(&mut hc)).await.expect("в период грации сервер ещё должен отвечать, в том числе на проверки здоровья");
    assert_eq!(during, ServingStatus::NotServing, "после сигнала остановки состояние должно стать NOT_SERVING сразу");
    tokio::time::sleep(ms(50)).await;
    within("новый вызов во время грации", work.run(RunRequest { ms: 10 })).await.expect("в период грации новые вызовы ещё принимаются (балансировщик мог не успеть заметить)");
    let long_took = within("долгий вызов", long).await.unwrap().expect("вызов, начатый до остановки, должен доработать до конца");
    assert!(long_took >= ms(550), "вызов на 600 мс закончился за {long_took:?}");
    let (result, took) = within("остановка", server).await.unwrap();
    result.expect("остановка должна кончаться Ok");
    assert!(took >= ms(550), "run вернулась через {took:?}, раньше конца долгого вызова");
    assert!(took < Duration::from_secs(3), "остановка затянулась: {took:?}");
    let mut late = WorkClient::new(Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_timeout(ms(300)).connect_lazy());
    let err = within("вызов после остановки", late.run(RunRequest { ms: 1 })).await.expect_err("после остановки порт закрыт");
    assert_eq!(err.code(), Code::Unavailable);
}

#[tokio::test]
async fn dropping_the_sender_also_stops_the_server() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (stop, signal) = oneshot::channel::<()>();
    let server = tokio::spawn(run(listener, signal, ms(100)));
    tokio::time::sleep(ms(100)).await;
    drop(stop);
    within("остановка", server).await.unwrap().expect("остановка должна кончаться Ok");
}
