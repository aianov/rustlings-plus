use std::time::Duration;

use notifier::{Closed, Config, Notifier, Recorder};
use tokio::time::{Instant, advance};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

async fn settle() {
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
}

fn config() -> Config {
    Config { window: ms(100), max: 3 }
}

fn texts(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[tokio::test(start_paused = true)]
async fn notifications_within_the_window_merge_in_arrival_order() {
    let sink = Recorder::new();
    let (hub, _runner) = Notifier::start(sink.clone(), config());
    hub.notify(1, "a").await.unwrap();
    hub.notify(1, "b").await.unwrap();
    settle().await;
    assert!(sink.deliveries().is_empty(), "пока окно не закрылось, ничего не уходит");
    advance(ms(100)).await;
    settle().await;
    assert_eq!(sink.deliveries(), vec![(1, texts(&["a", "b"]))], "обе записи уходят одним дайджестом в порядке прихода");
}

#[tokio::test(start_paused = true)]
async fn the_window_counts_from_the_first_notification() {
    let sink = Recorder::new();
    let (hub, _runner) = Notifier::start(sink.clone(), config());
    hub.notify(1, "a").await.unwrap();
    settle().await;
    advance(ms(60)).await;
    hub.notify(1, "b").await.unwrap();
    settle().await;
    advance(ms(39)).await;
    settle().await;
    assert!(sink.deliveries().is_empty(), "на 99-й миллисекунде окно ещё открыто");
    advance(ms(1)).await;
    settle().await;
    assert_eq!(sink.deliveries(), vec![(1, texts(&["a", "b"]))], "окно закрывается через 100 мс после первой записи, а не после последней");
}

#[tokio::test(start_paused = true)]
async fn a_full_digest_leaves_at_once_and_not_before() {
    let sink = Recorder::new();
    let (hub, _runner) = Notifier::start(sink.clone(), config());
    hub.notify(1, "a").await.unwrap();
    hub.notify(1, "b").await.unwrap();
    settle().await;
    assert!(sink.deliveries().is_empty(), "две записи из трёх: ещё не полон");
    hub.notify(1, "c").await.unwrap();
    settle().await;
    assert_eq!(sink.deliveries(), vec![(1, texts(&["a", "b", "c"]))], "третья запись заполнила дайджест: он уходит сразу, не дожидаясь окна");
}

#[tokio::test(start_paused = true)]
async fn after_a_delivery_the_next_notification_opens_a_fresh_window() {
    let sink = Recorder::new();
    let (hub, _runner) = Notifier::start(sink.clone(), config());
    hub.notify(1, "a").await.unwrap();
    settle().await;
    advance(ms(100)).await;
    settle().await;
    hub.notify(1, "b").await.unwrap();
    settle().await;
    advance(ms(100)).await;
    settle().await;
    assert_eq!(sink.deliveries(), vec![(1, texts(&["a"])), (1, texts(&["b"]))], "второй дайджест отдельный и со своим окном");
}

#[tokio::test(start_paused = true)]
async fn users_are_independent() {
    let sink = Recorder::new();
    let (hub, _runner) = Notifier::start(sink.clone(), config());
    hub.notify(1, "a").await.unwrap();
    settle().await;
    advance(ms(50)).await;
    hub.notify(2, "x").await.unwrap();
    settle().await;
    advance(ms(50)).await;
    settle().await;
    assert_eq!(sink.deliveries(), vec![(1, texts(&["a"]))], "у пользователя 1 окно закрылось, у пользователя 2 ещё нет");
    advance(ms(50)).await;
    settle().await;
    assert_eq!(sink.deliveries(), vec![(1, texts(&["a"])), (2, texts(&["x"]))], "пользователь 2 получает своё через 100 мс от своей первой записи");
}

#[tokio::test(start_paused = true)]
async fn muting_drops_the_pending_digest_and_new_notifications() {
    let sink = Recorder::new();
    let (hub, _runner) = Notifier::start(sink.clone(), config());
    hub.notify(1, "a").await.unwrap();
    hub.mute(1).await.unwrap();
    hub.notify(1, "b").await.unwrap();
    settle().await;
    advance(ms(500)).await;
    settle().await;
    assert!(sink.deliveries().is_empty(), "заглушённому не уходит ни накопленное, ни новое");
    hub.unmute(1).await.unwrap();
    hub.notify(1, "c").await.unwrap();
    settle().await;
    advance(ms(100)).await;
    settle().await;
    assert_eq!(sink.deliveries(), vec![(1, texts(&["c"]))], "после unmute уведомления снова идут");
}

#[tokio::test(start_paused = true)]
async fn finish_delivers_pending_digests_without_waiting() {
    let sink = Recorder::new();
    let (hub, runner) = Notifier::start(sink.clone(), config());
    hub.notify(2, "x").await.unwrap();
    hub.notify(1, "a").await.unwrap();
    let started = Instant::now();
    runner.finish().await;
    assert_eq!(started.elapsed(), Duration::ZERO, "остановка не должна ждать окна");
    assert_eq!(sink.deliveries(), vec![(1, texts(&["a"])), (2, texts(&["x"]))], "накопленное доставлено при остановке, по порядку пользователей");
}

#[tokio::test(start_paused = true)]
async fn notifying_a_finished_service_is_an_error() {
    let sink = Recorder::new();
    let (hub, runner) = Notifier::start(sink, config());
    runner.finish().await;
    assert_eq!(hub.notify(1, "a").await, Err(Closed), "после остановки notify обязан вернуть Closed");
}

#[tokio::test(start_paused = true)]
async fn finished_digests_do_not_leave_state_behind() {
    let sink = Recorder::new();
    let (hub, _runner) = Notifier::start(sink, config());
    for user in 0..5 {
        hub.notify(user, "a").await.unwrap();
    }
    settle().await;
    assert_eq!(hub.tracked_users().await.unwrap(), 5, "пять открытых дайджестов");
    advance(ms(100)).await;
    settle().await;
    assert_eq!(hub.tracked_users().await.unwrap(), 0, "после доставки сервис не помнит пользователей");
}
