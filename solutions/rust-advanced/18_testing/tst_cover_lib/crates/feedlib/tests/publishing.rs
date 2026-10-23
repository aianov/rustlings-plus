mod common;

use common::feed_at;
use feedlib::FeedError;

#[test]
fn cooldown_reports_remaining_seconds() {
    let (mut feed, clock) = feed_at(0);
    feed.publish("ann", "раз").unwrap();
    clock.advance(10);
    assert_eq!(feed.publish("ann", "два"), Err(FeedError::TooFast { wait: 20 }));
}

#[test]
fn cooldown_boundary() {
    let (mut feed, clock) = feed_at(0);
    feed.publish("ann", "раз").unwrap();
    clock.advance(29);
    assert_eq!(feed.publish("ann", "два"), Err(FeedError::TooFast { wait: 1 }));
    clock.advance(1);
    assert!(feed.publish("ann", "два").is_ok(), "ровно через 30 секунд можно");
}

#[test]
fn cooldown_is_per_author() {
    let (mut feed, _clock) = feed_at(0);
    feed.publish("ann", "раз").unwrap();
    assert!(feed.publish("bob", "два").is_ok(), "пауза Ани не касается Бори");
}

#[test]
fn cooldown_counts_from_the_last_post() {
    let (mut feed, clock) = feed_at(0);
    feed.publish("ann", "раз").unwrap();
    clock.advance(100);
    feed.publish("ann", "два").unwrap();
    clock.advance(10);
    assert_eq!(feed.publish("ann", "три"), Err(FeedError::TooFast { wait: 20 }), "отсчёт идёт от последнего поста, а не от первого");
}

#[test]
fn author_is_trimmed_before_comparison() {
    let (mut feed, _clock) = feed_at(0);
    feed.publish("  ann ", "раз").unwrap();
    assert!(matches!(feed.publish("ann", "два"), Err(FeedError::TooFast { .. })));
    assert_eq!(feed.page(None, None, 5)[0].author, "ann", "автор хранится без пробелов");
}
