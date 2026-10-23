use ntx_cancel::pb::publisher_client::PublisherClient;
use ntx_cancel::pb::PublishRequest;
use ntx_cancel::{start, State, Store};
use std::time::Duration;
use tonic::transport::{Channel, Endpoint};
use tonic::{Code, Request};

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

async fn setup() -> (PublisherClient<Channel>, std::sync::Arc<Store>) {
    let store = Store::new();
    let addr = start(store.clone()).await.unwrap();
    (PublisherClient::new(Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_lazy()), store)
}

fn publish(text: &str, timeout_ms: Option<u64>) -> Request<PublishRequest> {
    let mut request = Request::new(PublishRequest { text: text.to_string() });
    if let Some(ms) = timeout_ms {
        request.set_timeout(Duration::from_millis(ms));
    }
    request
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(500)).await;
}

#[tokio::test]
async fn patient_client_gets_the_id() {
    let (mut client, store) = setup().await;
    let reply = within("вызов", client.publish(publish("hi", None))).await.unwrap().into_inner();
    assert_eq!(reply.id, 1);
    assert_eq!(store.snapshot(), vec![("hi".to_string(), State::Published)]);
    assert_eq!(store.notified(), 1);
}

#[tokio::test]
async fn client_timeout_does_not_leave_the_post_half_done() {
    let (mut client, store) = setup().await;
    let err = within("вызов", client.publish(publish("hi", Some(30)))).await.expect_err("срок 30 мс меньше рассылки: клиент должен уйти");
    assert!(matches!(err.code(), Code::Cancelled | Code::DeadlineExceeded), "ждали обрыв по сроку, получили {err}");
    settle().await;
    assert_eq!(store.snapshot(), vec![("hi".to_string(), State::Published)], "клиент ушёл, а пост остался недоделанным: работа должна идти в отдельной задаче");
    assert_eq!(store.notified(), 1, "рассылка должна выполниться ровно один раз");
}

#[tokio::test]
async fn dropping_the_client_future_is_survived_too() {
    let (client, store) = setup().await;
    let mut c = client.clone();
    let call = tokio::spawn(async move { c.publish(publish("bye", None)).await });
    tokio::time::sleep(Duration::from_millis(40)).await;
    call.abort();
    settle().await;
    assert_eq!(store.snapshot(), vec![("bye".to_string(), State::Published)], "отмена на стороне клиента не должна ломать пост");
}

#[tokio::test]
async fn five_cancelled_calls_publish_five_posts_once_each() {
    let (client, store) = setup().await;
    let mut tasks = Vec::new();
    for i in 0..5 {
        let mut c = client.clone();
        tasks.push(tokio::spawn(async move { c.publish(publish(&format!("p{i}"), Some(30))).await }));
    }
    for task in tasks {
        let _ = within("вызов", task).await;
    }
    settle().await;
    let snap = store.snapshot();
    assert_eq!(snap.len(), 5);
    assert!(snap.iter().all(|(_, s)| *s == State::Published), "не все посты доведены до конца: {snap:?}");
    assert_eq!(store.notified(), 5, "каждая рассылка должна пройти один раз");
}

#[tokio::test]
async fn panic_in_the_job_is_an_internal_error_and_the_service_lives_on() {
    let (mut client, store) = setup().await;
    let err = within("вызов", client.publish(publish("boom", None))).await.expect_err("рассылка упала");
    assert_eq!(err.code(), Code::Internal, "паника в задаче должна стать Internal, а не обрывом соединения: {err}");
    let reply = within("вызов", client.publish(publish("fine", None))).await.expect("после паники сервис должен жить дальше");
    assert_eq!(reply.into_inner().id, 2);
    settle().await;
    assert_eq!(store.snapshot()[1], ("fine".to_string(), State::Published));
}
