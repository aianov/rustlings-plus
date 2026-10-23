use prost::Message as _;
use proto::CHAT;
use proto::social::v1::feed_event::Kind;
use proto::social::v1::{
    AvatarChunk, CaughtUp, CreatePostRequest, DeleteAvatarRequest, FeedEvent, HistoryRequest,
    Message, PairRequest, Post, PostAction, SendRequest, TypingRequest, WatchRequest, chat_server,
    posts_server, users_server,
};

#[test]
fn chat_endpoint_and_service_names() {
    assert_eq!(
        (CHAT.env, CHAT.default),
        ("CHAT_GRPC_ADDR", "127.0.0.1:50054")
    );
    assert_eq!(chat_server::SERVICE_NAME, "social.v1.Chat");
    assert_eq!(posts_server::SERVICE_NAME, "social.v1.Posts");
    assert_eq!(users_server::SERVICE_NAME, "social.v1.Users");
    let _ = (
        TypingRequest::default(),
        DeleteAvatarRequest {},
        HistoryRequest::default(),
    );
    let sent = SendRequest {
        ttl_seconds: 0,
        peer_id: 2,
        text: "привет".to_string(),
    };
    let message = Message {
        from_id: 1,
        to_id: sent.peer_id,
        text: sent.text,
        ..Message::default()
    };
    assert_eq!(message.to_id, 2);
}

#[test]
fn actors_moved_out_of_the_body() {
    let follow = PairRequest { target_id: 7 };
    let bytes = follow.encode_to_vec();
    assert_eq!(
        bytes,
        [0x10, 7],
        "в PairRequest остался только target_id под номером 2: номер 1 зарезервирован, кто действует, едет в x-user-id"
    );
    let _ = (
        CreatePostRequest::default(),
        PostAction { post_id: 1 },
        AvatarChunk::default(),
    );
}

#[test]
fn feed_event_is_a_oneof() {
    let post = Post {
        id: 1,
        text: "раз".to_string(),
        ..Post::default()
    };
    let events = [
        FeedEvent {
            kind: Some(Kind::History(post.clone())),
        },
        FeedEvent {
            kind: Some(Kind::CaughtUp(CaughtUp {})),
        },
        FeedEvent {
            kind: Some(Kind::Live(post)),
        },
    ];
    for event in events {
        let back =
            FeedEvent::decode(event.encode_to_vec().as_slice()).expect("protobuf разбирается");
        assert_eq!(back, event, "событие ленты переживает провод");
    }
    assert_eq!(
        WatchRequest::default().recent,
        0,
        "0 значит «сколько по умолчанию»"
    );
}
