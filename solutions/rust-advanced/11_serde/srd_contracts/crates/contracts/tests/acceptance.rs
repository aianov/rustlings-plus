use contracts::*;
use serde_json::{Value, json};

fn text<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap()
}

fn envelope(method: &str, data: &str) -> String {
    format!(r#"{{"id":9,"service":"x","method":"{method}","data":{data}}}"#)
}

#[test]
fn ids_are_prefixed_strings() {
    assert_eq!(text(&UserId(42)), r#""u_42""#);
    assert_eq!(text(&PostId(7)), r#""p_7""#);
    assert_eq!(serde_json::from_str::<UserId>(r#""u_42""#).unwrap(), UserId(42));
    assert_eq!(serde_json::from_str::<PostId>(r#""p_7""#).unwrap(), PostId(7));
    assert_eq!(text(&UserId(9_007_199_254_740_993)), r#""u_9007199254740993""#, "большой id не должен терять точность");
}

#[test]
fn bad_ids_explain_the_format() {
    for raw in [r#""42""#, r#""u_""#, r#""u_x""#, r#""p_1""#, r#""u_-1""#] {
        let err = serde_json::from_str::<UserId>(raw).expect_err("такой user id принимать нельзя");
        assert!(err.to_string().contains(&format!("invalid user id {raw}: expected u_<number>")), "{raw}: {err}");
    }
    assert!(serde_json::from_str::<PostId>(r#""u_1""#).unwrap_err().to_string().contains("expected p_<number>"));
    assert!(serde_json::from_str::<UserId>("42").is_err(), "число вместо строки");
}

#[test]
fn request_is_parsed_and_serialized_back() {
    let raw = envelope("users.register", r#"{"name":"ira"}"#);
    let request = parse_request(&raw).unwrap();
    assert_eq!((request.id, request.service.as_str(), request.method.as_str()), (9, "x", "users.register"));
    assert_eq!(request.data, json!({"name": "ira"}));
    let again: Value = serde_json::from_str(&text(&request)).unwrap();
    assert_eq!(again, serde_json::from_str::<Value>(&raw).unwrap());
}

#[test]
fn broken_requests_are_told_apart() {
    assert_eq!(parse_request(r#"{"id":"#), Err(ContractError::Truncated));
    assert_eq!(parse_request(r#"{"id": x}"#), Err(ContractError::Syntax { line: 1, column: 8 }));
    assert_eq!(
        parse_request(r#"{"id":1,"service":"s","data":{}}"#),
        Err(ContractError::BadEnvelope { reason: "missing field `method`".into() })
    );
    match parse_request(r#"{"id":"one","service":"s","method":"m","data":{}}"#) {
        Err(ContractError::BadEnvelope { reason }) => assert!(reason.starts_with("invalid type"), "{reason}"),
        other => panic!("ждали BadEnvelope, получили {other:?}"),
    }
}

#[test]
fn every_method_is_decoded() {
    let call = |method: &str, data: &str| parse_call(&envelope(method, data)).unwrap();
    assert_eq!(call("users.register", r#"{"name":"ira"}"#), (9, Call::Register { name: "ira".into() }));
    assert_eq!(call("posts.create", r#"{"text":"hi"}"#).1, Call::CreatePost { text: "hi".into() });
    assert_eq!(call("users.follow", r#"{"user":"u_7"}"#).1, Call::Follow { user: UserId(7) });
    assert_eq!(call("chat.send", r#"{"chat":3,"text":"yo"}"#).1, Call::ChatSend { chat: 3, text: "yo".into() });
}

#[test]
fn unknown_methods_and_bad_data_are_errors_not_panics() {
    assert_eq!(parse_call(&envelope("users.explode", "{}")), Err(ContractError::UnknownMethod("users.explode".into())));
    assert_eq!(
        parse_call(&envelope("users.register", r#"{"name":5}"#)),
        Err(ContractError::BadData { method: "users.register".into(), reason: "invalid type: integer `5`, expected a string".into() })
    );
    match parse_call(&envelope("users.follow", r#"{"user":"9"}"#)) {
        Err(ContractError::BadData { method, reason }) => {
            assert_eq!(method, "users.follow");
            assert!(reason.contains("invalid user id \"9\""), "{reason}");
        }
        other => panic!("ждали BadData, получили {other:?}"),
    }
    assert!(matches!(parse_call(&envelope("chat.send", "null")), Err(ContractError::BadData { .. })));
    assert_eq!(parse_call("{"), Err(ContractError::Truncated));
}

#[test]
fn responses_are_flat() {
    let ok = Response { id: 1, outcome: Outcome::Ok(json!({"post": "p_7"})) };
    assert_eq!(text(&ok), r#"{"id":1,"ok":{"post":"p_7"}}"#);
    let failed: Response<Value> =
        Response { id: 2, outcome: Outcome::Error(ErrorBody { code: "conflict".into(), message: "name taken".into() }) };
    assert_eq!(text(&failed), r#"{"id":2,"error":{"code":"conflict","message":"name taken"}}"#);
    let back: Response<Vec<PostId>> = serde_json::from_str(r#"{"id":3,"ok":["p_1","p_2"]}"#).unwrap();
    assert_eq!(back, Response { id: 3, outcome: Outcome::Ok(vec![PostId(1), PostId(2)]) });
    let back: Response<Value> = serde_json::from_str(&text(&failed)).unwrap();
    assert_eq!(back, failed);
}

#[test]
fn events_carry_a_type_tag() {
    let event = Event::PostCreated { post: PostId(7), author: UserId(1) };
    assert_eq!(text(&event), r#"{"type":"post_created","post":"p_7","author":"u_1"}"#);
    let event = Event::Followed { follower: UserId(2), followee: UserId(3) };
    assert_eq!(text(&event), r#"{"type":"followed","follower":"u_2","followee":"u_3"}"#);
    let raw = r#"{"type":"chat_message","chat":4,"from":"u_5","text":"hi"}"#;
    let parsed: Event = serde_json::from_str(raw).unwrap();
    assert_eq!(parsed, Event::ChatMessage { chat: 4, from: UserId(5), text: "hi".into() });
    assert_eq!(text(&parsed), raw);
    assert!(serde_json::from_str::<Event>(r#"{"type":"explosion"}"#).is_err());
}

#[test]
fn generic_envelope_works_for_any_payload() {
    assert_eq!(encode(5, &[UserId(1), UserId(2)]), r#"{"id":5,"data":["u_1","u_2"]}"#);
    let back = decode::<Vec<UserId>>(r#"{"id":5,"data":["u_1","u_2"]}"#).unwrap();
    assert_eq!(back, Envelope { id: 5, data: vec![UserId(1), UserId(2)] });
    assert_eq!(decode::<u32>(r#"{"id":5,"data":"x"#), Err(ContractError::Truncated));
    assert!(matches!(decode::<u32>(r#"{"id":5,"data":"x"}"#), Err(ContractError::BadEnvelope { .. })));
}

#[test]
fn errors_speak_human_and_machine() {
    let cases = [
        (ContractError::Truncated, "truncated", "message is incomplete"),
        (ContractError::Syntax { line: 2, column: 5 }, "syntax", "invalid JSON at line 2 column 5"),
        (ContractError::BadEnvelope { reason: "missing field `id`".into() }, "bad_envelope", "bad envelope: missing field `id`"),
        (ContractError::UnknownMethod("a.b".into()), "unknown_method", "unknown method a.b"),
        (ContractError::BadData { method: "chat.send".into(), reason: "oops".into() }, "bad_data", "bad data for chat.send: oops"),
    ];
    for (error, code, message) in cases {
        assert_eq!(error.to_string(), message);
        assert_eq!(error.to_body(), ErrorBody { code: code.into(), message: message.into() });
    }
    let reply: Response<Value> = Response { id: 1, outcome: Outcome::Error(ContractError::UnknownMethod("x.y".into()).to_body()) };
    assert_eq!(text(&reply), r#"{"id":1,"error":{"code":"unknown_method","message":"unknown method x.y"}}"#);
}

#[test]
fn the_error_is_a_normal_std_error() {
    fn takes<E: std::error::Error + Send + Sync + 'static>(_: E) {}
    takes(ContractError::Truncated);
}
