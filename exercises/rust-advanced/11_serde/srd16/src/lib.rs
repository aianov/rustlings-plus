// Строка чата разбирается без копирования: `from` и `text` берутся срезами из пришедшего JSON. Всё работало,
// пока пользователи писали «привет». Потом кто-то отправил сообщение с переводом строки, и сервер ответил
// `invalid type: string "a\nb", expected a borrowed string`. Заимствовать можно только то, что в исходнике
// лежит без изменений, а экранированный символ в нём записан иначе, чем в готовой строке. Нужно, чтобы
// обычные сообщения по-прежнему не копировались, а сложные читались с копированием.
// TODO: Исправь `ChatLine` так, чтобы разбирались оба вида сообщений.
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ChatLine<'a> {
    pub from: &'a str,
    pub text: &'a str,
}

pub fn parse_line(raw: &str) -> Result<ChatLine<'_>, serde_json::Error> {
    serde_json::from_str(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside(raw: &str, piece: &str) -> bool {
        raw.as_bytes().as_ptr_range().contains(&piece.as_ptr())
    }

    #[test]
    fn plain_text_is_not_copied() {
        let raw = r#"{"from":"ira","text":"hello"}"#;
        let line = parse_line(raw).unwrap();
        assert_eq!(line.text.to_string(), "hello");
        assert!(inside(raw, &line.text), "обычное сообщение должно оставаться срезом исходной строки");
        assert!(inside(raw, line.from));
    }

    #[test]
    fn newlines_and_quotes_are_fine() {
        let line = parse_line(r#"{"from":"ira","text":"a\nb"}"#).unwrap();
        assert_eq!(line.text.to_string(), "a\nb");
        let line = parse_line(r#"{"from":"ira","text":"she said \"hi\""}"#).unwrap();
        assert_eq!(line.text.to_string(), "she said \"hi\"");
        let line = parse_line(r#"{"from":"ira","text":"A\\"}"#).unwrap();
        assert_eq!(line.text.to_string(), "A\\");
    }

    #[test]
    fn missing_fields_still_fail() {
        assert!(parse_line(r#"{"from":"ira"}"#).is_err());
    }
}
