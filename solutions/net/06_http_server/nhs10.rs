use serde::Deserialize;
use serde::de::DeserializeOwned;

#[derive(Debug, Deserialize, PartialEq, Clone)]
#[allow(dead_code)]
struct Signup {
    name: String,
    age: u8,
}

fn parse_json<T: DeserializeOwned>(content_type: Option<&str>, body: &[u8]) -> Result<T, (u16, String)> {
    let unsupported = || (415, "Content-Type must be application/json (UTF-8)".to_string());
    let content_type = content_type.ok_or_else(unsupported)?;
    let mut parts = content_type.split(';');
    if !parts.next().unwrap_or("").trim().eq_ignore_ascii_case("application/json") {
        return Err(unsupported());
    }
    for param in parts {
        if let Some((name, value)) = param.split_once('=') {
            if name.trim().eq_ignore_ascii_case("charset") && !value.trim().trim_matches('"').eq_ignore_ascii_case("utf-8") {
                return Err(unsupported());
            }
        }
    }
    serde_json::from_slice(body).map_err(|e| (400, format!("invalid JSON: {e}")))
}

fn main() {
    let body = r#"{"name":"Ада","age":36}"#.as_bytes();
    println!("{:?}", parse_json::<Signup>(Some("application/json"), body));
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &[u8] = r#"{"name":"Ада","age":36}"#.as_bytes();

    #[test]
    fn accepts_good_requests() {
        let expected = Signup { name: "Ада".to_string(), age: 36 };
        for ct in ["application/json", "Application/JSON", "application/json; charset=utf-8", "application/json;charset=\"UTF-8\"", " application/json ;  charset=Utf-8 "] {
            assert_eq!(parse_json::<Signup>(Some(ct), GOOD), Ok(expected.clone()), "Content-Type {ct:?} подходит");
        }
    }

    #[test]
    fn wrong_media_type_is_415_even_for_empty_body() {
        for ct in [None, Some("text/plain"), Some("application/jsonp"), Some("application/x-www-form-urlencoded"), Some("application/json; charset=latin1"), Some("text/json"), Some("")] {
            for body in [GOOD, b"".as_slice()] {
                let e = parse_json::<Signup>(ct, body).unwrap_err();
                assert_eq!(e.0, 415, "Content-Type {ct:?}: сначала проверяется тип, и только потом тело");
            }
        }
    }

    #[test]
    fn broken_json_is_400_with_position() {
        for body in [&b""[..], b"{", b"{\"name\":", b"not json", b"{\"name\":\"a\",\"age\":1} trailing", b"{\"name\":\"\xff\",\"age\":1}"] {
            let (code, message) = parse_json::<Signup>(Some("application/json"), body).unwrap_err();
            assert_eq!(code, 400, "{:?} не JSON", String::from_utf8_lossy(body));
            assert!(!message.is_empty());
        }
        let (_, message) = parse_json::<Signup>(Some("application/json"), b"{\"name\": oops}").unwrap_err();
        assert!(message.contains("line 1"), "сообщение должно говорить, где ошибка: {message:?}");
    }

    #[test]
    fn wrong_shape_is_400_and_names_the_field() {
        let (code, message) = parse_json::<Signup>(Some("application/json"), r#"{"name":"Ада"}"#.as_bytes()).unwrap_err();
        assert_eq!(code, 400);
        assert!(message.contains("age"), "должно быть сказано, чего не хватает: {message:?}");
        for body in [&br#"{"name":"a","age":300}"#[..], br#"{"name":5,"age":1}"#, br#"[1,2]"#, b"null"] {
            let (code, _) = parse_json::<Signup>(Some("application/json"), body).unwrap_err();
            assert_eq!(code, 400, "{:?}: корректный JSON не той формы тоже 400", String::from_utf8_lossy(body));
        }
    }
}
