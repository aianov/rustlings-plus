use contracts::{
    Call, CommentId, CommentView, DEFAULT_NOTICES, Envelope, MAX_NOTICES, NoticeAsk, NoticeKind,
    NoticeView, PostId, Push, UserId, decode_call,
};
use serde_json::{Value, json};

fn wire<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("вид сериализуется")
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
