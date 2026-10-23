// Идентификатор поста это 64-битное число, и однажды сервер выдал такое: `9007199254740993`. JavaScript-клиент
// хранит все числа как дробные двойной точности и целые точно только до 2^53, поэтому прочитал его как
// `9007199254740992`, и лайк ушёл в чужой пост. Сервер тут ни при чём, JSON это допускает: проблема в том,
// что мы отправили большое целое числом. На проводе идентификатор должен быть строкой, а читать нужно
// и строку, и число (старые записи в кэше клиентов остались числами).
// TODO: Сделай так, чтобы `id` ходил строкой, а принимался и строкой, и числом.
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Post {
    pub id: i64,
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const BIG: i64 = 9_007_199_254_740_993;

    fn js_reads(json: &str) -> i64 {
        let value: Value = serde_json::from_str(json).unwrap();
        match &value["id"] {
            Value::Number(n) => n.as_f64().unwrap() as i64,
            Value::String(s) => s.parse().unwrap(),
            other => panic!("странный id: {other}"),
        }
    }

    #[test]
    fn a_js_client_reads_the_id_exactly() {
        let json = serde_json::to_string(&Post { id: BIG, text: "hi".into() }).unwrap();
        assert_eq!(js_reads(&json), BIG, "клиент прочитал другой id: большое целое нельзя отправлять числом ({json})");
    }

    #[test]
    fn id_travels_as_a_string() {
        let json = serde_json::to_string(&Post { id: 42, text: "x".into() }).unwrap();
        assert_eq!(json, r#"{"id":"42","text":"x"}"#);
    }

    #[test]
    fn both_forms_are_accepted() {
        let text: Post = serde_json::from_str(r#"{"id":"9007199254740993","text":"a"}"#).unwrap();
        assert_eq!(text.id, BIG);
        let number: Post = serde_json::from_str(r#"{"id":5,"text":"a"}"#).unwrap();
        assert_eq!(number.id, 5, "числа из старого кэша ещё встречаются");
        let negative: Post = serde_json::from_str(r#"{"id":"-5","text":"a"}"#).unwrap();
        assert_eq!(negative.id, -5);
    }

    #[test]
    fn garbage_ids_are_errors() {
        assert!(serde_json::from_str::<Post>(r#"{"id":"abc","text":"a"}"#).is_err());
        assert!(serde_json::from_str::<Post>(r#"{"id":true,"text":"a"}"#).is_err());
        assert!(serde_json::from_str::<Post>(r#"{"id":"","text":"a"}"#).is_err());
    }
}
