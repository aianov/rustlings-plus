use contracts::{
    ANONYMOUS_METHODS, Envelope, Error, METHOD_NAMES, PostId, Push, SESSION_METHODS, SessionCall,
    UserId, decode, decode_session, encode, needs_actor,
};
use serde_json::{Value, json};

fn envelope(method: &str, data: Value) -> Envelope {
    Envelope::new(7, method, data)
}

#[test]
fn session_methods_parse() {
    assert_eq!(
        SESSION_METHODS,
        ["session.hello", "session.bye"],
        "у сессии два метода"
    );
    let hello = decode_session(&envelope("session.hello", json!({"username": "Alice"})));
    let Ok(SessionCall::Hello { username }) = hello else {
        panic!("session.hello с ником это Hello, а пришло {hello:?}");
    };
    assert_eq!(
        username.as_ref(),
        "alice",
        "ник в hello нормализуется, как везде"
    );
    assert_eq!(
        decode_session(&envelope("session.bye", json!({}))),
        Ok(SessionCall::Bye),
        "bye без данных"
    );
    assert!(
        matches!(
            decode_session(&envelope("session.hello", json!({}))),
            Err(Error::BadData { .. })
        ),
        "hello без ника это bad_data"
    );
    assert!(
        matches!(
            decode_session(&envelope("session.hello", json!({"username": "ali ce"}))),
            Err(Error::BadData { .. })
        ),
        "кривой ник это тоже bad_data"
    );
    assert_eq!(
        decode_session(&envelope("session.dance", json!({}))),
        Err(Error::UnknownMethod(String::from("session.dance"))),
        "других методов у сессии нет"
    );
    let mut wrong = envelope("session.bye", json!({}));
    wrong.service = String::from("users");
    assert!(
        matches!(decode_session(&wrong), Err(Error::WrongService { .. })),
        "сервис в конверте обязан совпадать с началом метода"
    );
}

#[test]
fn only_register_and_find_go_without_a_session() {
    assert_eq!(
        ANONYMOUS_METHODS,
        ["users.register", "users.find"],
        "без сессии можно только зарегистрироваться и найти пользователя"
    );
    for method in METHOD_NAMES {
        assert_eq!(
            needs_actor(method),
            !ANONYMOUS_METHODS.contains(&method),
            "{method}: действует от чьего-то имени, значит нужна сессия"
        );
    }
    for method in SESSION_METHODS {
        assert!(!needs_actor(method), "{method} сам создаёт сессию");
    }
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
