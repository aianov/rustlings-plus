mod common;

use common::feed_at;

#[test]
fn newest_first() {
    let (mut feed, clock) = feed_at(0);
    for author in ["a", "b", "c"] {
        feed.publish(author, "текст").unwrap();
        clock.advance(31);
    }
    let ids: Vec<u32> = feed.page(None, None, 10).iter().map(|p| p.id).collect();
    assert_eq!(ids, vec![3, 2, 1]);
}

#[test]
fn before_excludes_the_cursor_and_size_limits() {
    let (mut feed, _clock) = feed_at(0);
    for author in ["a", "b", "c", "d", "e"] {
        feed.publish(author, "текст").unwrap();
    }
    let ids: Vec<u32> = feed.page(None, Some(4), 2).iter().map(|p| p.id).collect();
    assert_eq!(ids, vec![3, 2], "пост 4 уже был на прошлой странице");
}

#[test]
fn author_filter_ignores_case() {
    let (mut feed, clock) = feed_at(0);
    feed.publish("Ann", "раз").unwrap();
    feed.publish("bob", "два").unwrap();
    clock.advance(31);
    feed.publish("ANN", "три").unwrap();
    let ids: Vec<u32> = feed.page(Some("ann"), None, 10).iter().map(|p| p.id).collect();
    assert_eq!(ids, vec![3, 1]);
}

#[test]
fn summary_counts_posts_and_distinct_authors() {
    let (mut feed, clock) = feed_at(0);
    feed.publish("ann", "раз").unwrap();
    feed.publish("bob", "два").unwrap();
    clock.advance(31);
    feed.publish("ann", "три").unwrap();
    assert_eq!(feed.summary(), "постов: 3, авторов: 2");
}
