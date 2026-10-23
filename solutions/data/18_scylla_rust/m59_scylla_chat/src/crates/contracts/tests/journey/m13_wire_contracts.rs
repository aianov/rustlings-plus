use std::collections::HashSet;

use contracts::{
    Call, Changed, ChatHistory, DEFAULT_FEED_LIMIT, DEFAULT_HISTORY_LIMIT, Envelope, Error,
    ErrorBody, FeedPage, METHOD_NAMES, MessageView, Outcome, PostId, PostView, Push, Reply,
    Response, UserId, UserView, Username, decode, decode_call, write_line,
};
use serde_json::{Value, json};

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("нормальный ник проходит проверку")
}

fn wire<T: serde::Serialize>(value: &T) -> Value {
    let mut out = Vec::new();
    write_line(&mut out, value).expect("запись в Vec не ломается");
    let text = String::from_utf8(out).expect("write_line пишет UTF-8");
    assert!(
        text.ends_with('\n') && text.matches('\n').count() == 1,
        "write_line пишет ровно одну строку и заканчивает её '\\n', а записал {text:?}"
    );
    serde_json::from_str(&text).expect("write_line пишет настоящий JSON")
}

fn envelope(method: &str, data: Value) -> Envelope {
    Envelope::new(1, method, data)
}

fn all_calls() -> Vec<Call> {
    vec![
        Call::Register {
            username: name("alice"),
        },
        Call::FindUser {
            username: name("bob"),
        },
        Call::Follow {
            user: UserId(2),
            target: UserId(1),
        },
        Call::Mute {
            user: UserId(2),
            target: UserId(1),
        },
        Call::Profile { target: UserId(1) },
        Call::CreatePost {
            user: UserId(1),
            text: String::from("Привет #rust"),
            tags: Vec::new(),
            links: Vec::new(),
        },
        Call::Like {
            user: UserId(2),
            post: PostId(1),
        },
        Call::EditPost {
            user: UserId(1),
            post: PostId(1),
            text: String::from("исправлено"),
        },
        Call::DeletePost {
            user: UserId(1),
            post: PostId(1),
        },
        Call::Feed {
            user: UserId(2),
            page: 2,
            size: 5,
        },
        Call::Comment {
            user: UserId(2),
            post: PostId(1),
            text: String::from("согласен"),
        },
        Call::SendMessage {
            user: UserId(1),
            peer: UserId(2),
            text: String::from("привет"),
        },
        Call::History {
            user: UserId(1),
            peer: UserId(2),
            limit: 30,
        },
        Call::Unread { user: UserId(1) },
        Call::Notices {
            user: UserId(1),
            limit: 5,
        },
        Call::ReadNotices { user: UserId(1) },
    ]
}

#[test]
fn ids_travel_as_prefixed_strings() {
    assert_eq!(UserId(42).to_string(), "u_42", "Display для UserId");
    assert_eq!(PostId(7).to_string(), "p_7", "Display для PostId");
    assert_eq!(
        wire(&UserId(42)),
        json!("u_42"),
        "UserId на проводе строка u_<число>"
    );
    assert_eq!(
        wire(&PostId(7)),
        json!("p_7"),
        "PostId на проводе строка p_<число>"
    );
    assert_eq!(
        "u_42".parse::<UserId>(),
        Ok(UserId(42)),
        "FromStr для UserId"
    );
    assert_eq!("p_7".parse::<PostId>(), Ok(PostId(7)), "FromStr для PostId");
    assert_eq!(
        serde_json::from_value::<UserId>(json!("u_18446744073709551615")).ok(),
        Some(UserId(u64::MAX)),
        "UserId читается из строки, весь диапазон u64 доступен"
    );
    assert_eq!(
        serde_json::from_value::<PostId>(json!("p_0")).ok(),
        Some(PostId(0)),
        "PostId читается из строки"
    );
}

#[test]
fn bad_ids_are_rejected_with_a_clear_reason() {
    for text in [
        "7",
        "u_",
        "u_x",
        "u_+5",
        "u_ 5",
        "u_-1",
        "p_1",
        "U_1",
        "u_18446744073709551616",
    ] {
        let error = text
            .parse::<UserId>()
            .expect_err("кривой id пользователя не разбирается");
        assert_eq!(
            error.to_string(),
            format!("неверный id {text:?}: нужно u_<число>"),
            "текст IdError для {text:?}"
        );
    }
    let error = "u_1"
        .parse::<PostId>()
        .expect_err("id пользователя не годится в id поста");
    assert_eq!(
        error.to_string(),
        "неверный id \"u_1\": нужно p_<число>",
        "текст IdError для поста"
    );
    assert!(
        serde_json::from_value::<UserId>(json!(7)).is_err(),
        "число вместо строки не годится: id на проводе всегда строка с префиксом"
    );
}

#[test]
fn username_travels_as_a_plain_string() {
    assert_eq!(
        wire(&name("alice")),
        json!("alice"),
        "Username на проводе просто строка, без @"
    );
    assert_eq!(
        serde_json::from_value::<Username>(json!(" Bob ")).ok(),
        Some(name("bob")),
        "при чтении ник нормализуется так же, как в Username::try_from"
    );
    let error =
        serde_json::from_value::<Username>(json!("   ")).expect_err("пустой ник не читается");
    assert_eq!(
        error.to_string(),
        "ник не может быть пустым",
        "причина отказа это текст NameError"
    );
}

#[test]
fn envelope_takes_service_from_method() {
    let request = Envelope::new(
        5,
        "users.register",
        Call::Register {
            username: name("alice"),
        },
    );
    assert_eq!(
        request.service, "users",
        "service это часть метода до точки"
    );
    assert_eq!(
        wire(&request),
        json!({
            "id": 5,
            "service": "users",
            "method": "users.register",
            "data": {"username": "alice"}
        }),
        "конверт запроса: id, service, method и data"
    );
}

#[test]
fn method_names_are_fixed() {
    assert_eq!(
        METHOD_NAMES,
        [
            "users.register",
            "users.find",
            "users.follow",
            "users.mute",
            "users.profile",
            "posts.create",
            "posts.like",
            "posts.edit",
            "posts.delete",
            "posts.feed",
            "posts.comment",
            "chat.send",
            "chat.history",
            "notify.unread",
            "notify.list",
            "notify.read",
        ],
        "METHOD_NAMES перечисляет методы в порядке из ТЗ"
    );
    let methods: HashSet<&str> = all_calls().iter().map(Call::method).collect();
    assert_eq!(
        methods,
        METHOD_NAMES.into_iter().collect(),
        "Call::method называет каждый метод из METHOD_NAMES, и у каждого варианта свой"
    );
}

#[test]
fn request_data_has_the_documented_shape() {
    let shapes = [
        json!({"username": "alice"}),
        json!({"username": "bob"}),
        json!({"user": "u_2", "target": "u_1"}),
        json!({"user": "u_2", "target": "u_1"}),
        json!({"target": "u_1"}),
        json!({"user": "u_1", "text": "Привет #rust"}),
        json!({"user": "u_2", "post": "p_1"}),
        json!({"user": "u_1", "post": "p_1", "text": "исправлено"}),
        json!({"user": "u_1", "post": "p_1"}),
        json!({"user": "u_2", "page": 2, "size": 5}),
        json!({"user": "u_2", "post": "p_1", "text": "согласен"}),
        json!({"user": "u_1", "peer": "u_2", "text": "привет"}),
        json!({"user": "u_1", "peer": "u_2", "limit": 30}),
        json!({"user": "u_1"}),
        json!({"user": "u_1", "limit": 5}),
        json!({"user": "u_1"}),
    ];
    for (call, shape) in all_calls().iter().zip(shapes) {
        assert_eq!(
            wire(call),
            shape,
            "данные метода {} на проводе (Call пишется как объект своих полей)",
            call.method()
        );
    }
}

#[test]
fn every_call_survives_the_wire() {
    for (id, call) in (1..).zip(all_calls()) {
        let mut line = Vec::new();
        write_line(&mut line, &Envelope::new(id, call.method(), &call))
            .expect("запрос записывается");
        let line = String::from_utf8(line).expect("запрос в UTF-8");
        let request: Envelope = decode(&line).expect("свой же конверт разбирается");
        assert_eq!(request.id, id, "id конверта доезжает как есть");
        assert_eq!(
            decode_call(&request),
            Ok(call.clone()),
            "{} туда и обратно даёт тот же Call",
            call.method()
        );
    }
}

#[test]
fn optional_fields_get_defaults() {
    assert_eq!(
        decode_call(&envelope("posts.feed", json!({"user": "u_2"}))),
        Ok(Call::Feed {
            user: UserId(2),
            page: 1,
            size: DEFAULT_FEED_LIMIT,
        }),
        "posts.feed без page и size: первая страница размером DEFAULT_FEED_LIMIT"
    );
    assert_eq!(
        decode_call(&envelope(
            "chat.history",
            json!({"user": "u_1", "peer": "u_2"})
        )),
        Ok(Call::History {
            user: UserId(1),
            peer: UserId(2),
            limit: DEFAULT_HISTORY_LIMIT,
        }),
        "chat.history без limit берёт DEFAULT_HISTORY_LIMIT"
    );
}

#[test]
fn history_limit_is_checked() {
    for limit in [0, 101] {
        assert_eq!(
            decode_call(&envelope(
                "chat.history",
                json!({"user": "u_1", "peer": "u_2", "limit": limit})
            )),
            Err(Error::BadData {
                method: String::from("chat.history"),
                reason: format!("лимит истории от 1 до 100, а не {limit}"),
            }),
            "limit {limit} вне 1..=100"
        );
    }
    assert!(
        decode_call(&envelope(
            "chat.history",
            json!({"user": "u_1", "peer": "u_2", "limit": 100})
        ))
        .is_ok(),
        "limit 100 допустим"
    );
}

#[test]
fn first_stage_errors_are_told_apart() {
    let cases = [
        (
            "{\"id\": 1",
            Error::Truncated,
            "truncated",
            "конверт оборван",
        ),
        ("", Error::Truncated, "truncated", "конверт оборван"),
        (
            "{\"id\": 1,,}",
            Error::Syntax {
                line: 1,
                column: 10,
            },
            "syntax",
            "не JSON (строка 1, столбец 10)",
        ),
        (
            "nope",
            Error::Syntax { line: 1, column: 2 },
            "syntax",
            "не JSON (строка 1, столбец 2)",
        ),
        (
            "{\"id\": 1, \"service\": \"users\", \"method\": \"users.find\"}",
            Error::BadEnvelope {
                reason: String::from("missing field `data`"),
            },
            "bad_envelope",
            "кривой конверт: missing field `data`",
        ),
        (
            "{\"id\": \"1\", \"service\": \"users\", \"method\": \"users.find\", \"data\": {}}",
            Error::BadEnvelope {
                reason: String::from("invalid type: string \"1\", expected u64"),
            },
            "bad_envelope",
            "кривой конверт: invalid type: string \"1\", expected u64",
        ),
    ];
    for (raw, expected, code, text) in cases {
        let error = decode::<Envelope>(raw).expect_err("кривой конверт не разбирается");
        assert_eq!(error, expected, "вид ошибки для {raw:?}");
        assert_eq!(error.code(), code, "код ошибки для {raw:?}");
        assert_eq!(
            error.to_string(),
            text,
            "текст ошибки для {raw:?}: причина без хвоста про строку и столбец"
        );
    }
}

#[test]
fn unknown_method_comes_before_wrong_service() {
    let unknown = decode_call(&envelope("users.fly", json!({}))).expect_err("метода нет");
    assert_eq!(unknown, Error::UnknownMethod(String::from("users.fly")));
    assert_eq!(unknown.code(), "unknown_method");
    assert_eq!(unknown.to_string(), "неизвестный метод users.fly");

    let mut request = envelope("users.register", json!({"username": "alice"}));
    request.service = String::from("posts");
    let wrong = decode_call(&request).expect_err("метод не из этого сервиса");
    assert_eq!(
        wrong,
        Error::WrongService {
            service: String::from("posts"),
            method: String::from("users.register"),
        }
    );
    assert_eq!(wrong.code(), "wrong_service");
    assert_eq!(
        wrong.to_string(),
        "метод users.register не из сервиса posts"
    );

    let mut request = envelope("posts.fly", json!({}));
    request.service = String::from("users");
    assert_eq!(
        decode_call(&request),
        Err(Error::UnknownMethod(String::from("posts.fly"))),
        "неизвестный метод проверяется раньше сервиса"
    );
}

#[test]
fn bad_data_names_the_method_and_the_reason() {
    let cases = [
        (
            "users.follow",
            json!({"user": "7", "target": "u_1"}),
            "неверный id \"7\": нужно u_<число>",
        ),
        (
            "posts.like",
            json!({"user": "u_1", "post": "u_1"}),
            "неверный id \"u_1\": нужно p_<число>",
        ),
        ("users.register", json!({}), "missing field `username`"),
        (
            "users.register",
            json!({"username": ""}),
            "ник не может быть пустым",
        ),
        (
            "users.find",
            json!({"username": "abcdefghijklmnopqrstuvwxyz"}),
            "ник длиннее 20 символов",
        ),
        (
            "chat.send",
            json!({"user": "u_1", "peer": "u_2"}),
            "missing field `text`",
        ),
    ];
    for (method, data, reason) in cases {
        let error = decode_call(&envelope(method, data)).expect_err("данные не по форме");
        assert_eq!(
            error,
            Error::BadData {
                method: String::from(method),
                reason: String::from(reason),
            },
            "BadData для {method}"
        );
        assert_eq!(error.code(), "bad_data");
        assert_eq!(
            error.to_string(),
            format!("плохие данные для {method}: {reason}"),
            "текст BadData"
        );
    }
    assert!(
        matches!(
            decode_call(&envelope(
                "users.follow",
                json!({"user": 2, "target": "u_1"})
            )),
            Err(Error::BadData { .. })
        ),
        "число вместо id это BadData"
    );
}

#[test]
fn replies_have_the_documented_shape() {
    let post = PostView {
        liked: false,
        latest: Vec::new(),
        id: PostId(1),
        author: UserId(1),
        author_name: String::from("alice"),
        text: String::from("Привет #rust"),
        likes: 2,
        tags: Vec::new(),
        links: Vec::new(),
    };
    let message = MessageView {
        from: UserId(1),
        to: UserId(2),
        text: String::from("привет"),
    };
    let post_json = json!({
        "id": "p_1", "author": "u_1", "author_name": "alice", "text": "Привет #rust", "likes": 2
    });
    let message_json = json!({"from": "u_1", "to": "u_2", "text": "привет"});
    let cases = [
        (
            Reply::User(UserView {
                id: UserId(1),
                username: String::from("alice"),
                display_name: None,
                bio: None,
                avatar: None,
            }),
            json!({"id": "u_1", "username": "alice"}),
        ),
        (Reply::Post(post.clone()), post_json.clone()),
        (
            Reply::Changed(Changed { changed: true }),
            json!({"changed": true}),
        ),
        (
            Reply::Feed(FeedPage {
                posts: vec![post],
                ..FeedPage::default()
            }),
            json!({ "posts": [post_json] }),
        ),
        (Reply::Message(message.clone()), message_json.clone()),
        (
            Reply::History(ChatHistory {
                messages: vec![message],
                next: None,
            }),
            json!({ "messages": [message_json] }),
        ),
    ];
    for (reply, shape) in cases {
        assert_eq!(
            wire(&reply),
            shape,
            "Reply пишется как сам ответ, без имени варианта: {reply:?}"
        );
    }
}

#[test]
fn responses_are_flat() {
    let ok = Response {
        id: Some(3),
        outcome: Outcome::Ok(Reply::Changed(Changed { changed: false })),
        request_id: None,
    };
    assert_eq!(
        wire(&ok),
        json!({"id": 3, "ok": {"changed": false}}),
        "успех: {{\"id\", \"ok\"}} в одном объекте"
    );
    assert_eq!(
        wire(&ok.clone().with_request_id("gw-41")),
        json!({"id": 3, "ok": {"changed": false}, "request_id": "gw-41"}),
        "номер запроса лежит рядом с ответом, когда шлюз его знает"
    );
    let failed: Response<Reply> = Response {
        id: None,
        outcome: Outcome::Error(ErrorBody {
            code: String::from("truncated"),
            message: String::from("конверт оборван"),
            details: Vec::new(),
            request_id: None,
        }),
        request_id: None,
    };
    assert_eq!(
        wire(&failed),
        json!({"id": null, "error": {"code": "truncated", "message": "конверт оборван"}}),
        "отказ: {{\"id\", \"error\": {{\"code\", \"message\"}}}}, id может быть null"
    );
}

#[test]
fn client_decodes_typed_responses() {
    assert_eq!(
        decode::<Response<UserView>>(r#"{"id": 1, "ok": {"id": "u_1", "username": "alice"}}"#),
        Ok(Response {
            id: Some(1),
            outcome: Outcome::Ok(UserView {
                id: UserId(1),
                username: String::from("alice"),
                display_name: None,
                bio: None,
                avatar: None,
            }),
            request_id: None,
        }),
        "клиент читает ответ сразу в нужный тип: Response<T> обобщённый"
    );
    assert_eq!(
        decode::<Response<FeedPage>>(
            r#"{"id": null, "error": {"code": "syntax", "message": "не JSON"}}"#
        ),
        Ok(Response {
            id: None,
            outcome: Outcome::Error(ErrorBody {
                code: String::from("syntax"),
                message: String::from("не JSON"),
                details: Vec::new(),
                request_id: None,
            }),
            request_id: None,
        }),
        "отказ читается в любой Response<T>"
    );
    assert!(
        matches!(
            decode::<Response<UserView>>(r#"{"id": 1, "ok": {"id": "u_1"}}"#),
            Err(Error::BadEnvelope { .. })
        ),
        "ответ не той формы это BadEnvelope"
    );
}

#[test]
fn pushes_carry_a_tag() {
    let cases = [
        (
            Push::Followed {
                to: UserId(1),
                by: UserId(2),
            },
            json!({"event": "followed", "data": {"to": "u_1", "by": "u_2"}}),
        ),
        (
            Push::Liked {
                to: UserId(1),
                by: UserId(2),
                post: PostId(3),
            },
            json!({"event": "liked", "data": {"to": "u_1", "by": "u_2", "post": "p_3"}}),
        ),
        (
            Push::Message {
                to: UserId(2),
                from: UserId(1),
                seq: 7,
                text: String::from("привет"),
            },
            json!({"event": "message", "data": {"to": "u_2", "from": "u_1", "seq": 7, "text": "привет"}}),
        ),
    ];
    for (push, shape) in cases {
        assert_eq!(wire(&push), shape, "push-событие: имя в event, поля в data");
        assert_eq!(
            decode::<Push>(&shape.to_string()),
            Ok(push),
            "push читается обратно"
        );
    }
}

#[test]
fn write_line_keeps_one_message_per_line() {
    let post = PostView {
        liked: false,
        latest: Vec::new(),
        id: PostId(1),
        author: UserId(1),
        author_name: String::from("alice"),
        text: String::from("первая\nвторая"),
        likes: 0,
        tags: Vec::new(),
        links: Vec::new(),
    };
    assert_eq!(
        wire(&post)["text"],
        json!("первая\nвторая"),
        "перевод строки внутри текста экранируется, сообщение остаётся одной строкой"
    );
}

fn contract_value<T: Clone + PartialEq + Eq + Send + Sync + std::error::Error + 'static>() {}

#[test]
fn contract_error_is_a_plain_value() {
    contract_value::<Error>();
    let parsed: contracts::Result<Envelope> = decode("{");
    assert_eq!(
        parsed.map(|request| request.id),
        Err(Error::Truncated),
        "contracts::Result<T> это Result<T, contracts::Error>"
    );
    assert_eq!(
        wire(&ErrorBody {
            code: String::from("bad_data"),
            message: String::from("плохие данные"),
            details: Vec::new(),
            request_id: None,
        }),
        json!({"code": "bad_data", "message": "плохие данные"}),
        "ErrorBody на проводе"
    );
}
