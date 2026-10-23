use contracts::{
    Call, CommentId, CommentView, DEFAULT_NOTICES, Envelope, Event, EventBody, EventKind,
    MAX_NOTICES, NoticeAsk, NoticeKind, NoticeView, NotifyMethod, Origin, PostId, Push, Topics,
    UserId, decode_call,
};
use serde_json::{Value, json};

fn wire<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("вид сериализуется")
}

#[test]
fn the_subject_schema_is_fixed() {
    let topics = Topics::new("social");
    let (alice, bob, post) = (UserId(1), UserId(2), PostId(7));
    let cases = [
        (
            Event::followed(alice, bob),
            "social.events.user.followed.u_2",
        ),
        (
            Event::post_created(alice, post),
            "social.events.post.created.u_1",
        ),
        (
            Event::post_liked(bob, alice, post),
            "social.events.post.liked.u_1",
        ),
        (
            Event::post_commented(bob, alice, post),
            "social.events.post.commented.u_1",
        ),
    ];
    for (event, subject) in cases {
        assert_eq!(
            topics.event(&event).join("."),
            subject,
            "тема события: корень, events, сущность, действие и владелец: {event:?}"
        );
        assert_eq!(
            topics.read_event(subject),
            Some((event.kind, event.owner)),
            "тема читается обратно в вид и владельца"
        );
    }
    assert_eq!(topics.all_events().join("."), "social.events.>");
    assert_eq!(topics.notice(bob).join("."), "social.notices.u_2");
    assert_eq!(topics.all_notices().join("."), "social.notices.*");
    assert_eq!(topics.read_notice("social.notices.u_2"), Some(bob));
    let methods: Vec<String> = NotifyMethod::ALL
        .into_iter()
        .map(|method| topics.notify(method).join("."))
        .collect();
    assert_eq!(
        methods,
        [
            "social.notify.unread",
            "social.notify.list",
            "social.notify.read"
        ],
        "запросы к notify-service: корень, notify и метод, без пользователя в теме"
    );
}

#[test]
fn foreign_or_crooked_subjects_are_not_ours() {
    let topics = Topics::new("social");
    for subject in [
        "other.events.user.followed.u_2",
        "social.notices.user.followed.u_2",
        "social.events.user.liked.u_2",
        "social.events.post.followed.u_2",
        "social.events.user.followed.u_02",
        "social.events.user.followed.2",
        "social.events.user.followed",
        "social.events.user.followed.u_2.extra",
    ] {
        assert_eq!(
            topics.read_event(subject),
            None,
            "{subject}: чужой корень, незнакомое действие или неканонический id это не наше событие"
        );
    }
    assert_eq!(topics.read_notice("social.notices.u_01"), None);
    assert_eq!(topics.read_notice("social.notify.u_1"), None);
    assert_eq!(
        EventKind::of("post", "commented"),
        Some(EventKind::PostCommented)
    );
    assert_eq!(EventKind::of("user", "created"), None);
}

#[test]
fn an_event_body_is_small_and_has_no_owner() {
    let followed = Event::followed(UserId(1), UserId(2));
    assert_eq!(
        wire(&followed.body),
        json!({"actor": "u_1"}),
        "у подписки нет поста, владелец живёт в теме"
    );
    let liked = Event::post_liked(UserId(2), UserId(1), PostId(7));
    assert_eq!(wire(&liked.body), json!({"actor": "u_2", "post": "p_7"}));
    assert_eq!(
        serde_json::from_value::<EventBody>(json!({"actor": "u_3"})).expect("тело без поста"),
        EventBody {
            actor: UserId(3),
            post: None
        }
    );
    assert!(
        serde_json::from_value::<EventBody>(json!({"actor": "alice"})).is_err(),
        "актор это id пользователя, а не ник"
    );
    let origin = Origin::new("e856e880e9a85e6d").traced(Some("trace-1"));
    assert_eq!(
        (origin.request_id.as_str(), origin.trace_id.as_deref()),
        ("e856e880e9a85e6d", Some("trace-1")),
        "Origin несёт номер запроса и трассу туда, где родилось событие"
    );
}

#[test]
fn a_notice_becomes_the_old_push() {
    let notice = |kind, post| NoticeView {
        id: 1,
        to: UserId(1),
        kind,
        actor: UserId(2),
        post,
        read: false,
    };
    let post = Some(PostId(7));
    assert_eq!(
        notice(NoticeKind::Followed, None).push(),
        Some(Push::Followed {
            to: UserId(1),
            by: UserId(2)
        })
    );
    assert_eq!(
        notice(NoticeKind::Liked, post).push(),
        Some(Push::Liked {
            to: UserId(1),
            by: UserId(2),
            post: PostId(7)
        })
    );
    assert_eq!(
        notice(NoticeKind::NewPost, post).push(),
        Some(Push::NewPost {
            to: UserId(1),
            author: UserId(2),
            post: PostId(7)
        }),
        "у нового поста актор это автор"
    );
    assert_eq!(
        notice(NoticeKind::Commented, None).push(),
        None,
        "уведомление о посте без поста не пушится"
    );
    assert_eq!(
        wire(&Push::Commented {
            to: UserId(1),
            by: UserId(2),
            post: PostId(7)
        }),
        json!({"event": "commented", "data": {"to": "u_1", "by": "u_2", "post": "p_7"}}),
        "новый push о комментарии в том же формате, что и остальные"
    );
    assert_eq!(
        wire(&notice(NoticeKind::NewPost, post)),
        json!({"id": 1, "to": "u_1", "kind": "new_post", "actor": "u_2", "post": "p_7", "read": false})
    );
    assert_eq!(
        wire(&NoticeAsk {
            user: UserId(1),
            limit: None
        }),
        json!({"user": "u_1"}),
        "запрос к notify-service без limit не пишет его"
    );
    assert_eq!(
        wire(&CommentView {
            id: CommentId(4),
            post: PostId(7)
        }),
        json!({"id": "c_4", "post": "p_7"})
    );
}

#[test]
fn comments_and_the_bell_are_methods_of_the_contract() {
    let call = |method: &str, data: Value| decode_call(&Envelope::new(1, method, data));
    assert_eq!(
        call(
            "posts.comment",
            json!({"user": "u_1", "post": "p_7", "text": "согласен"})
        ),
        Ok(Call::Comment {
            user: UserId(1),
            post: PostId(7),
            text: String::from("согласен")
        })
    );
    assert_eq!(
        call("notify.unread", json!({"user": "u_1"})),
        Ok(Call::Unread { user: UserId(1) })
    );
    assert_eq!(
        call("notify.read", json!({"user": "u_1"})),
        Ok(Call::ReadNotices { user: UserId(1) })
    );
    assert_eq!(
        call("notify.list", json!({"user": "u_1"})),
        Ok(Call::Notices {
            user: UserId(1),
            limit: DEFAULT_NOTICES
        }),
        "по умолчанию {DEFAULT_NOTICES} последних"
    );
    for limit in [0, MAX_NOTICES + 1] {
        let refused = call("notify.list", json!({"user": "u_1", "limit": limit}))
            .expect_err("limit вне границ");
        assert_eq!(refused.kind(), "bad_data", "limit {limit}: {refused:?}");
    }
    let unread = Call::Unread { user: UserId(1) };
    assert_eq!(unread.method(), "notify.unread");
}
