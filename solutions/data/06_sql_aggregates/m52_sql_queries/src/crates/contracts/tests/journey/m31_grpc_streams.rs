use contracts::{
    DEFAULT_RECENT, Envelope, FeedPush, LIVE_METHODS, LiveCall, MAX_RECENT, PostId, PostView,
    UserId, decode, decode_live, encode,
};
use serde_json::{Value, json};

fn envelope(method: &str, data: Value) -> Envelope {
    let service = method.split('.').next().unwrap_or(method);
    decode(&json!({"id": 1, "service": service, "method": method, "data": data}).to_string())
        .expect("конверт разбирается")
}

#[test]
fn live_methods_decode_with_defaults_and_limits() {
    assert_eq!(LIVE_METHODS, ["posts.watch", "posts.unwatch"]);
    assert_eq!(
        decode_live(&envelope("posts.watch", json!({"user": "u_2"}))),
        Ok(LiveCall::Watch {
            user: UserId(2),
            recent: DEFAULT_RECENT
        })
    );
    assert_eq!(
        decode_live(&envelope(
            "posts.watch",
            json!({"user": "u_2", "recent": 7})
        )),
        Ok(LiveCall::Watch {
            user: UserId(2),
            recent: 7
        })
    );
    for recent in [0, MAX_RECENT + 1] {
        let refused = decode_live(&envelope(
            "posts.watch",
            json!({"user": "u_2", "recent": recent}),
        ));
        assert!(
            matches!(&refused, Err(error) if error.code() == "bad_data"),
            "recent {recent} вне 1..={MAX_RECENT}: {refused:?}"
        );
    }
    assert_eq!(
        decode_live(&envelope("posts.unwatch", json!({"user": "u_2"}))),
        Ok(LiveCall::Unwatch { user: UserId(2) })
    );
    let other = decode_live(&envelope("posts.feed", json!({"user": "u_2"})));
    assert!(
        matches!(&other, Err(error) if error.code() == "unknown_method"),
        "decode_live знает только живые методы: {other:?}"
    );
    let wrong = decode_live(
        &decode::<Envelope>(
            &json!({"id": 1, "service": "users", "method": "posts.watch", "data": {"user": "u_1"}})
                .to_string(),
        )
        .expect("конверт"),
    );
    assert!(
        matches!(&wrong, Err(error) if error.code() == "wrong_service"),
        "{wrong:?}"
    );
}

#[test]
fn feed_events_are_tagged_like_pushes() {
    let post = PostView {
        liked: false,
        id: PostId(3),
        author: UserId(1),
        author_name: "alice".to_string(),
        text: "привет".to_string(),
        likes: 0,
        tags: Vec::new(),
        links: Vec::new(),
    };
    let live = FeedPush::FeedPost {
        to: UserId(2),
        post,
        live: true,
    };
    let wire: Value = serde_json::from_str(&encode(&live).expect("кодируется")).expect("JSON");
    assert_eq!(
        wire,
        json!({"event": "feed_post", "data": {"to": "u_2", "post": {"id": "p_3", "author": "u_1", "author_name": "alice", "text": "привет", "likes": 0}, "live": true}}),
        "событие ленты как push: {{event, data}} без id"
    );
    let caught = encode(&FeedPush::FeedCaughtUp { to: UserId(2) }).expect("кодируется");
    assert_eq!(caught, r#"{"event":"feed_caught_up","data":{"to":"u_2"}}"#);
    assert_eq!(
        decode::<FeedPush>(&caught),
        Ok(FeedPush::FeedCaughtUp { to: UserId(2) })
    );
}
