use contracts::{
    DomainEvent, Fact, Followed, KINDS, MessageSent, Muted, PostCommented, PostCreated,
    PostDeleted, PostEdited, PostId, PostLiked, ProfileChanged, Registered, UserDeleted, UserId,
};
use e2e::golden;
use serde::Deserialize;

const AT: u64 = 1_700_000_000_000;

fn samples() -> Vec<Fact> {
    let (alice, bob) = (UserId(1), UserId(2));
    let post = PostId(10);
    vec![
        Fact::Registered(Registered {
            user_id: alice,
            username: String::from("alice"),
        }),
        Fact::ProfileChanged(ProfileChanged {
            user_id: alice,
            display_name: Some(String::from("Алиса")),
        }),
        Fact::Followed(Followed {
            follower_id: bob,
            followee_id: alice,
        }),
        Fact::Muted(Muted {
            user_id: bob,
            muted_id: alice,
        }),
        Fact::PostCreated(PostCreated {
            post_id: post,
            author_id: alice,
            author_name: String::from("alice"),
            text: String::from("привет #rust"),
            tags: vec![String::from("rust")],
            links: Vec::new(),
        }),
        Fact::PostLiked(PostLiked {
            post_id: post,
            author_id: alice,
            user_id: bob,
        }),
        Fact::PostCommented(PostCommented {
            post_id: post,
            author_id: alice,
            comment_id: contracts::CommentId(3),
            commenter_id: bob,
        }),
        Fact::PostEdited(PostEdited {
            post_id: post,
            text: String::from("привет, мир"),
        }),
        Fact::PostDeleted(PostDeleted { post_id: post }),
        Fact::MessageSent(MessageSent {
            from_id: alice,
            to_id: bob,
            text: String::from("как дела?"),
        }),
        Fact::UserDeleted(UserDeleted { user_id: bob }),
    ]
}

fn event(fact: &Fact) -> DomainEvent {
    DomainEvent::new(&format!("evt-{}", fact.kind()), fact, 1, AT)
}

fn file(kind: &str) -> String {
    format!("{kind}.json")
}

fn stored(kind: &str) -> DomainEvent {
    let path = std::path::Path::new(e2e::GOLDEN_DIR).join(file(kind));
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

#[test]
fn every_event_kind_has_a_golden_format() {
    let facts = samples();
    let kinds: Vec<&str> = facts.iter().map(Fact::kind).collect();
    assert_eq!(kinds, KINDS, "у каждого вида события есть образец");
    for fact in &facts {
        let json = serde_json::to_string_pretty(&event(fact)).expect("событие пишется в JSON");
        golden(&file(fact.kind()), &json);
    }
}

#[test]
fn golden_events_read_back_into_the_same_facts() {
    for fact in samples() {
        let event = stored(fact.kind());
        assert_eq!(
            event.fact().expect("золотое событие читается"),
            Some(fact.clone()),
            "{}",
            fact.kind()
        );
    }
}

#[derive(Debug, Deserialize)]
struct FeedNeedsPost {
    post_id: PostId,
    author_id: UserId,
    text: String,
}

#[derive(Debug, Deserialize)]
struct FeedNeedsFollow {
    follower_id: UserId,
    followee_id: UserId,
}

#[derive(Debug, Deserialize)]
struct NotifyNeedsLike {
    author_id: UserId,
    user_id: UserId,
}

#[derive(Debug, Deserialize)]
struct NotifyNeedsComment {
    author_id: UserId,
    commenter_id: UserId,
}

#[derive(Debug, Deserialize)]
struct FeedNeedsGone {
    user_id: UserId,
}

fn payload<T: serde::de::DeserializeOwned>(kind: &str) -> T {
    let event = stored(kind);
    serde_json::from_value(event.payload.clone()).unwrap_or_else(|error| {
        panic!("потребитель {kind} не находит нужных полей: {error}\n{event:?}")
    })
}

#[test]
fn consumers_find_the_fields_they_rely_on() {
    let post: FeedNeedsPost = payload("post.created");
    assert_eq!((post.post_id, post.author_id), (PostId(10), UserId(1)));
    assert!(!post.text.is_empty());
    let follow: FeedNeedsFollow = payload("user.followed");
    assert_eq!(
        (follow.follower_id, follow.followee_id),
        (UserId(2), UserId(1))
    );
    let like: NotifyNeedsLike = payload("post.liked");
    assert_ne!(like.author_id, like.user_id);
    let comment: NotifyNeedsComment = payload("post.commented");
    assert_ne!(comment.author_id, comment.commenter_id);
    let gone: FeedNeedsGone = payload("user.deleted");
    assert_eq!(gone.user_id, UserId(2));
}

#[test]
fn consumers_tolerate_new_fields_and_unknown_kinds() {
    let mut event = stored("post.created");
    if let serde_json::Value::Object(fields) = &mut event.payload {
        fields.insert(String::from("mood"), serde_json::json!("радость"));
    }
    let fact = event.fact().expect("лишнее поле не ломает чтение");
    assert!(matches!(fact, Some(Fact::PostCreated(..))));
    let mut future = stored("post.created");
    future.kind = String::from("post.pinned");
    assert_eq!(
        future.fact().expect("незнакомый вид не ошибка"),
        None,
        "незнакомое событие потребитель пропускает"
    );
    let mut newer = stored("post.created");
    newer.schema = 99;
    assert!(newer.fact().is_err(), "схему из будущего не угадываем");
}
