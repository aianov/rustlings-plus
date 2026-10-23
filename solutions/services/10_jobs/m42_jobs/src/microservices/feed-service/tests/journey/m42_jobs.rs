use std::sync::Arc;

use contracts::{
    DomainEvent, Fact, Followed, PostCreated, PostId, Registered, UserDeleted, UserId,
    fresh_event_id,
};
use feed_service::{FeedService, MemoryFeed};

fn at(version: u64, fact: Fact, when: u64) -> DomainEvent {
    DomainEvent::new(&fresh_event_id(), &fact, version, when)
}

fn registered(user: u64, name: &str) -> DomainEvent {
    at(
        1,
        Fact::Registered(Registered {
            user_id: UserId(user),
            username: name.to_string(),
        }),
        1,
    )
}

fn followed(version: u64, follower: u64, followee: u64) -> DomainEvent {
    at(
        version,
        Fact::Followed(Followed {
            follower_id: UserId(follower),
            followee_id: UserId(followee),
        }),
        2,
    )
}

fn posted(post: u64, author: u64, text: &str) -> DomainEvent {
    at(
        1,
        Fact::PostCreated(PostCreated {
            post_id: PostId(post),
            author_id: UserId(author),
            author_name: format!("ник{author}"),
            text: text.to_string(),
            tags: Vec::new(),
            links: Vec::new(),
        }),
        3 + post,
    )
}

fn deleted(version: u64, user: u64) -> DomainEvent {
    at(
        version,
        Fact::UserDeleted(UserDeleted {
            user_id: UserId(user),
        }),
        9,
    )
}

#[tokio::test]
async fn a_deleted_user_disappears_from_every_feed() {
    let feed = FeedService::new(Arc::new(MemoryFeed::new()));
    for event in [
        registered(1, "alice"),
        registered(2, "bob"),
        registered(3, "carol"),
        followed(2, 2, 1),
        followed(3, 2, 3),
        followed(2, 1, 3),
        posted(1, 1, "от alice"),
        posted(2, 3, "от carol"),
    ] {
        feed.consume(&event).await.expect("событие");
    }
    let texts = |items: Vec<feed_service::FeedItem>| -> Vec<String> {
        items.into_iter().map(|item| item.text).collect()
    };
    assert_eq!(
        texts(feed.page(UserId(2), 1, 10).await),
        ["от carol", "от alice"]
    );
    feed.consume(&deleted(3, 1)).await.expect("удаление");
    assert_eq!(
        texts(feed.page(UserId(2), 1, 10).await),
        ["от carol"],
        "посты удалённого автора пропали из лент подписчиков"
    );
    let snapshot = feed.snapshot().await;
    assert!(
        !snapshot.names.contains_key(&UserId(1)),
        "имени удалённого нет"
    );
    assert!(
        snapshot
            .follows
            .iter()
            .all(|(who, whom)| *who != UserId(1) && *whom != UserId(1)),
        "подписок удалённого и на удалённого нет: {:?}",
        snapshot.follows
    );
    assert!(
        texts(feed.page(UserId(1), 1, 10).await).is_empty(),
        "своей ленты у удалённого тоже нет"
    );
}
