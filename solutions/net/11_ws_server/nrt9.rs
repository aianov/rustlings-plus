// По одному WebSocket бегут запросы разных сервисов, и ответы приходят не по порядку. Чтобы клиент понял, на что пришёл ответ,
// каждый запрос несёт номер: {"id": 5, "service": "post", "method": "like", "data": {...}}. Ответ повторяет id и говорит, как дело.
// Клиенты бывают кривыми, и сервер обязан отвечать на мусор, а не падать. Главный вопрос в том, на какие ошибки ещё можно указать id.
// TODO: Допиши разбор запроса и сборку ответов (вид ответов смотри в тестах).

use serde_json::{Value, json};

#[derive(Debug, PartialEq)]
struct Request {
    id: u64,
    service: String,
    method: String,
    data: Value,
}

#[derive(Debug, PartialEq)]
struct Failure {
    id: Option<u64>,
    code: &'static str,
    message: String,
}

fn parse_request(text: &str) -> Result<Request, Failure> {
    let value: Value = serde_json::from_str(text).map_err(|e| Failure { id: None, code: "bad_json", message: e.to_string() })?;
    let Some(object) = value.as_object() else {
        return Err(Failure { id: None, code: "bad_request", message: "ожидался JSON-объект".to_string() });
    };
    let Some(id) = object.get("id").and_then(Value::as_u64) else {
        return Err(Failure { id: None, code: "bad_request", message: "id должен быть неотрицательным целым".to_string() });
    };
    let fail = |message: &str| Failure { id: Some(id), code: "bad_request", message: message.to_string() };
    let name = |field: &str| -> Result<String, Failure> {
        match object.get(field).and_then(Value::as_str) {
            Some(s) if !s.is_empty() && !s.contains('.') => Ok(s.to_string()),
            _ => Err(fail(&format!("поле {field} должно быть непустой строкой без точки"))),
        }
    };
    let service = name("service")?;
    let method = name("method")?;
    let data = object.get("data").cloned().unwrap_or(Value::Null);
    Ok(Request { id, service, method, data })
}

fn render_ok(id: u64, data: Value) -> String {
    json!({ "id": id, "ok": true, "data": data }).to_string()
}

fn render_err(failure: &Failure) -> String {
    json!({ "id": failure.id, "ok": false, "error": { "code": failure.code, "message": failure.message } }).to_string()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_request() {
        let r = parse_request(r#"{"id":5,"service":"post","method":"like","data":{"post":42}}"#).unwrap();
        assert_eq!(r, Request { id: 5, service: "post".into(), method: "like".into(), data: json!({"post": 42}) });
    }

    #[test]
    fn data_is_optional_and_extra_fields_are_ignored() {
        let r = parse_request(r#"{"id":0,"service":"user","method":"me","trace":"abc","extra":[1,2]}"#).unwrap();
        assert_eq!(r.data, Value::Null, "без data должно получиться null");
        assert_eq!(r.id, 0);
        let max = parse_request(&format!(r#"{{"id":{},"service":"a","method":"b"}}"#, u64::MAX)).unwrap();
        assert_eq!(max.id, u64::MAX, "id помещается в u64 целиком");
    }

    #[test]
    fn garbage_has_no_id_to_answer_with() {
        for text in ["", "не json", "[1,2,3]", "42", r#""строка""#, "null", r#"{"id":"#] {
            let f = parse_request(text).unwrap_err();
            assert_eq!(f.id, None, "для {text:?} отвечать с id нельзя: его нет");
            assert!(f.code == "bad_json" || f.code == "bad_request", "для {text:?} код {:?}", f.code);
        }
        assert_eq!(parse_request("не json").unwrap_err().code, "bad_json");
        assert_eq!(parse_request("[1]").unwrap_err().code, "bad_request");
    }

    #[test]
    fn bad_id_cannot_be_echoed() {
        for text in [
            r#"{"service":"a","method":"b"}"#,
            r#"{"id":"5","service":"a","method":"b"}"#,
            r#"{"id":-1,"service":"a","method":"b"}"#,
            r#"{"id":1.5,"service":"a","method":"b"}"#,
            r#"{"id":null,"service":"a","method":"b"}"#,
        ] {
            let f = parse_request(text).unwrap_err();
            assert_eq!((f.id, f.code), (None, "bad_request"), "для {text}");
        }
    }

    #[test]
    fn broken_fields_keep_the_id_so_the_client_can_match_the_error() {
        for text in [
            r#"{"id":7,"method":"b"}"#,
            r#"{"id":7,"service":"a"}"#,
            r#"{"id":7,"service":5,"method":"b"}"#,
            r#"{"id":7,"service":"","method":"b"}"#,
            r#"{"id":7,"service":"a.b","method":"c"}"#,
            r#"{"id":7,"service":"a","method":"b.c"}"#,
        ] {
            let f = parse_request(text).unwrap_err();
            assert_eq!((f.id, f.code), (Some(7), "bad_request"), "id известен, значит клиент должен получить ошибку именно на свой запрос: {text}");
        }
    }

    #[test]
    fn replies_are_single_line_json_with_the_right_shape() {
        let ok = render_ok(5, json!({"liked": true, "text": "строка\nс переводом"}));
        assert!(!ok.contains('\n'), "в текстовом сообщении перевод строки должен быть экранирован");
        let v: Value = serde_json::from_str(&ok).unwrap();
        assert_eq!(v, json!({"id": 5, "ok": true, "data": {"liked": true, "text": "строка\nс переводом"}}));

        let err = render_err(&Failure { id: Some(9), code: "not_found", message: "нет такого".into() });
        let v: Value = serde_json::from_str(&err).unwrap();
        assert_eq!(v, json!({"id": 9, "ok": false, "error": {"code": "not_found", "message": "нет такого"}}));

        let anonymous = render_err(&Failure { id: None, code: "bad_json", message: "x".into() });
        let v: Value = serde_json::from_str(&anonymous).unwrap();
        assert_eq!(v["id"], Value::Null, "ответ без известного id несёт null, а не пропущенное поле");
        assert_eq!(v["ok"], json!(false));
    }

    #[test]
    fn reply_round_trips_through_parse_for_a_client() {
        let text = render_ok(11, json!([1, 2, 3]));
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["id"].as_u64(), Some(11));
        assert_eq!(v["data"], json!([1, 2, 3]));
    }
}
