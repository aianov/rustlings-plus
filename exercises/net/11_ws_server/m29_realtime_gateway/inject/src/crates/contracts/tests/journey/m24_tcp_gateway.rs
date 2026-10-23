use contracts::{Envelope, Error, PostId, Push, UserId, decode, encode};
use serde_json::{Value, json};

fn envelope(method: &str, data: Value) -> Envelope {
    Envelope::new(7, method, data)
}

#[test]
fn act_as_fills_and_guards_the_user() {
    let mut post = envelope("posts.create", json!({"text": "привет"}));
    post.act_as(UserId(3)).expect("user подставляется");
    assert_eq!(
        post.data,
        json!({"user": "u_3", "text": "привет"}),
        "act_as вписывает user из сессии"
    );
    let mut same = envelope("posts.like", json!({"user": "u_3", "post": "p_1"}));
    assert_eq!(same.act_as(UserId(3)), Ok(()), "свой user в data разрешён");
    let mut other = envelope("posts.like", json!({"user": "u_4", "post": "p_1"}));
    let refused = other.act_as(UserId(3));
    assert!(
        matches!(&refused, Err(error @ Error::Forbidden { .. }) if error.code() == "forbidden"),
        "чужой user в data это forbidden, а не тихая подмена; пришло {refused:?}"
    );
    let mut odd = envelope("posts.like", json!([1, 2]));
    assert!(
        matches!(odd.act_as(UserId(3)), Err(Error::BadData { .. })),
        "data не объект: подставить user некуда"
    );
}

#[test]
fn encode_gives_one_json_line() {
    let push = Push::NewPost {
        to: UserId(2),
        author: UserId(1),
        post: PostId(5),
    };
    let line = encode(&push).expect("push кодируется");
    assert!(
        !line.contains('\n'),
        "encode даёт строку без перевода строки: {line:?}"
    );
    assert_eq!(
        decode::<Push>(&line),
        Ok(push),
        "encode и decode симметричны"
    );
}
