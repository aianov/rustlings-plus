use ntx_server_stream::pb::feed_client::FeedClient;
use ntx_server_stream::pb::{Event, SubscribeRequest};
use ntx_server_stream::{start, Hub};
use std::time::{Duration, Instant};
use tonic::transport::{Channel, Endpoint};
use tonic::Streaming;

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

async fn setup() -> (FeedClient<Channel>, std::sync::Arc<Hub>) {
    let hub = Hub::new(256);
    let addr = start(hub.clone()).await.unwrap();
    (FeedClient::new(Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_lazy()), hub)
}

fn event(user_id: u64, text: &str) -> Event {
    Event { user_id, text: text.to_string() }
}

async fn subscribe(client: &mut FeedClient<Channel>, user_id: u64) -> Streaming<Event> {
    within("подписка", client.subscribe(SubscribeRequest { user_id })).await.unwrap().into_inner()
}

async fn next_text(stream: &mut Streaming<Event>) -> String {
    within("чтение потока", stream.message()).await.unwrap().expect("поток закрылся раньше времени").text
}

#[tokio::test]
async fn only_own_events_in_publication_order() {
    let (mut client, hub) = setup().await;
    let mut stream = subscribe(&mut client, 1).await;
    for (user, text) in [(1, "a"), (2, "x"), (1, "b"), (2, "y"), (1, "c")] {
        hub.publish(event(user, text));
    }
    let got = vec![next_text(&mut stream).await, next_text(&mut stream).await, next_text(&mut stream).await];
    assert_eq!(got, vec!["a", "b", "c"], "должны прийти только события пользователя 1 и по порядку");
}

#[tokio::test]
async fn events_published_right_after_open_are_not_lost() {
    let (mut client, hub) = setup().await;
    let mut stream = subscribe(&mut client, 7).await;
    for i in 0..100 {
        hub.publish(event(7, &i.to_string()));
    }
    for i in 0..100 {
        assert_eq!(next_text(&mut stream).await, i.to_string(), "потеряно событие {i}: подписываться на хаб надо до возврата из обработчика");
    }
}

#[tokio::test]
async fn two_subscribers_of_one_user_see_the_same() {
    let (mut client, hub) = setup().await;
    let mut a = subscribe(&mut client, 3).await;
    let mut b = subscribe(&mut client, 3).await;
    hub.publish(event(3, "hello"));
    assert_eq!(next_text(&mut a).await, "hello");
    assert_eq!(next_text(&mut b).await, "hello");
}

#[tokio::test]
async fn closing_the_stream_unsubscribes_even_without_new_events() {
    let (mut client, hub) = setup().await;
    let stream = subscribe(&mut client, 1).await;
    let other = subscribe(&mut client, 2).await;
    assert_eq!(hub.subscribers(), 2);
    drop(stream);
    let started = Instant::now();
    while hub.subscribers() > 1 {
        assert!(started.elapsed() < Duration::from_secs(3), "клиент ушёл, а подписка на хаб осталась (подписчиков: {}): обработчик ждёт события вместо того, чтобы заметить закрытие потока", hub.subscribers());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    drop(other);
    while hub.subscribers() > 0 {
        assert!(started.elapsed() < Duration::from_secs(3), "подписки не снимаются");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
