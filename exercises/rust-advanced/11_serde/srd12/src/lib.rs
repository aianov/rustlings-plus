// Чат-сервис разбирает конверты через уже готовый `decode`, и в продакшене он дважды упал. Первый раз клиент
// прислал `chat.send` без текста, и поток разбора запаниковал на `unwrap`. Второй раз пришёл метод от
// будущей версии клиента, `chat.pin`, и поток упал в `panic!`. Кроме того, профилировщик показал, что `data`
// копируется целиком на каждый вызов, хотя разбору хватило бы ссылки на дерево.
// TODO: Сделай так, чтобы ни плохие данные, ни неизвестный метод не роняли поток, а возвращались ошибкой
// с понятным текстом, и разбор не копировал `data`.
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug)]
pub struct Request {
    pub id: u64,
    pub method: String,
    pub data: Value,
}

#[derive(Debug, PartialEq)]
pub enum Call {
    Send { chat: u64, text: String },
    History { chat: u64, limit: u32 },
}

#[derive(Debug, PartialEq)]
pub enum CallError {
    UnknownMethod(String),
    BadData { method: String, reason: String },
}

#[derive(Deserialize)]
struct SendData {
    chat: u64,
    text: String,
}

#[derive(Deserialize)]
struct HistoryData {
    chat: u64,
    limit: u32,
}

pub fn decode(request: &Request) -> Result<Call, CallError> {
    match request.method.as_str() {
        "chat.send" => {
            let data: SendData = serde_json::from_value(request.data.clone()).unwrap();
            Ok(Call::Send { chat: data.chat, text: data.text })
        }
        "chat.history" => {
            let data: HistoryData = serde_json::from_value(request.data.clone()).unwrap();
            Ok(Call::History { chat: data.chat, limit: data.limit })
        }
        other => panic!("unknown method {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SRC: &str = include_str!("lib.rs");

    fn request(method: &str, data: Value) -> Request {
        Request { id: 1, method: method.into(), data }
    }

    #[test]
    fn good_requests_decode() {
        let send = decode(&request("chat.send", json!({"chat": 4, "text": "hi"}))).unwrap();
        assert_eq!(send, Call::Send { chat: 4, text: "hi".into() });
        let history = decode(&request("chat.history", json!({"chat": 4, "limit": 50}))).unwrap();
        assert_eq!(history, Call::History { chat: 4, limit: 50 });
    }

    #[test]
    fn missing_text_is_an_error_not_a_panic() {
        let err = decode(&request("chat.send", json!({"chat": 4}))).unwrap_err();
        match err {
            CallError::BadData { method, reason } => {
                assert_eq!(method, "chat.send");
                assert!(reason.contains("missing field `text`"), "причина должна говорить, чего не хватает: {reason}");
            }
            other => panic!("ждали BadData, получили {other:?}"),
        }
    }

    #[test]
    fn wrong_types_are_errors_too() {
        let err = decode(&request("chat.history", json!({"chat": 4, "limit": "ten"}))).unwrap_err();
        assert!(matches!(&err, CallError::BadData { reason, .. } if reason.contains("invalid type")), "{err:?}");
        assert!(matches!(decode(&request("chat.send", Value::Null)), Err(CallError::BadData { .. })));
    }

    #[test]
    fn a_future_method_is_a_clean_error() {
        assert_eq!(decode(&request("chat.pin", json!({}))), Err(CallError::UnknownMethod("chat.pin".into())));
    }

    #[test]
    fn data_is_not_copied() {
        let code = SRC.split("#[cfg(test)]").next().unwrap();
        let compact: String = code.split_whitespace().collect();
        assert!(
            !compact.contains("data.clone(") && !compact.contains("data.to_owned(") && !compact.contains("Value::clone("),
            "не клонируй `data`: разбор должен идти по ссылке на дерево (клонировать `method` для текста ошибки можно)"
        );
    }
}
