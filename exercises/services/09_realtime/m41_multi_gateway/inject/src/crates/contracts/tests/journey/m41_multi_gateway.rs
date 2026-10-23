use contracts::{
    DomainEvent, Fact, KINDS, MAX_REVOKED, MessageSent, OnlineView, Push, REALTIME_METHODS,
    SessionRevoked, Topics, TypingNote, UserId,
};
use serde_json::json;

#[test]
fn realtime_topics_have_fixed_shapes() {
    let topics = Topics::new("social");
    let user = UserId(7);
    assert_eq!(topics.chat_stream(), "SOCIAL_CHAT");
    assert_eq!(topics.chat_subjects(), ["social.chat.>"]);
    assert_eq!(topics.chat("u_7").join("."), "social.chat.u_7");
    assert_eq!(topics.live(user).join("."), "social.live.u_7");
    assert_eq!(topics.live_all().join("."), "social.live.>");
    assert_eq!(topics.typing(user).join("."), "social.typing.u_7");
    assert_eq!(topics.revoked().join("."), "social.session.revoked");
    assert_eq!(
        topics.presence(),
        "SOCIAL_PRESENCE",
        "бакет присутствия по корню тем: тесты не мешают друг другу"
    );
    assert!(
        !topics
            .stream_subjects()
            .iter()
            .any(|subject| subject.starts_with("social.live")),
        "живая тема не ловится stream событий: повтор не зациклится"
    );
}

#[test]
fn a_chat_message_is_a_fact_about_its_recipient() {
    assert_eq!(KINDS.len(), 10);
    assert!(KINDS.contains(&"chat.sent"));
    let fact = Fact::MessageSent(MessageSent {
        from_id: UserId(1),
        to_id: UserId(2),
        text: String::from("привет"),
    });
    assert_eq!(
        (fact.kind(), fact.schema(), fact.aggregate()),
        ("chat.sent", 1, String::from("u_2")),
        "агрегат это получатель: версия считается по его входящим"
    );
    let event = DomainEvent::new("evt-1", &fact, 3, 42);
    assert_eq!(
        serde_json::to_value(&event).expect("JSON")["payload"],
        json!({"from_id": "u_1", "to_id": "u_2", "text": "привет"})
    );
    assert_eq!(event.fact(), Ok(Some(fact)));
}

#[test]
fn realtime_pushes_on_the_wire() {
    let message = Push::Message {
        to: UserId(2),
        from: UserId(1),
        seq: 41,
        text: String::from("привет"),
    };
    assert_eq!(
        serde_json::to_value(&message).expect("JSON"),
        json!({"event": "message", "data": {"to": "u_2", "from": "u_1", "seq": 41, "text": "привет"}}),
        "сообщение несёт текст и номер в stream: по номеру клиент догоняет пропущенное"
    );
    let typing = Push::Typing {
        to: UserId(2),
        from: UserId(1),
    };
    assert_eq!(
        serde_json::to_value(&typing).expect("JSON"),
        json!({"event": "typing", "data": {"to": "u_2", "from": "u_1"}})
    );
    let note = TypingNote {
        from: UserId(1),
        to: UserId(2),
    };
    assert_eq!(
        serde_json::to_value(note).expect("JSON"),
        json!({"from": "u_1", "to": "u_2"})
    );
    let revoked = SessionRevoked {
        user: UserId(1),
        sessions: vec![String::from("ab12-3")],
    };
    assert_eq!(
        serde_json::to_value(&revoked).expect("JSON"),
        json!({"user": "u_1", "sessions": ["ab12-3"]})
    );
    let online = OnlineView {
        user: UserId(2),
        online: true,
    };
    assert_eq!(
        serde_json::to_value(online).expect("JSON"),
        json!({"user": "u_2", "online": true})
    );
    assert_eq!(
        MAX_REVOKED, 1000,
        "шлюз помнит не больше тысячи отозванных сессий"
    );
    assert_eq!(
        REALTIME_METHODS,
        ["chat.typing", "users.online"],
        "у шлюза два новых метода: «печатает» и «в сети»"
    );
}
