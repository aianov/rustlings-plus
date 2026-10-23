// Фронтенд на `app.example.com` ходит к API на `api.example.com`, и браузер требует CORS-заголовков. Разработчик упёрся в ошибку «нельзя
// использовать `*` вместе с credentials» и «починил» так: сервер подставляет в `Access-Control-Allow-Origin` тот Origin, который пришёл, и
// всегда добавляет `Access-Control-Allow-Credentials: true`. Теперь любой чужой сайт может в браузере вошедшего пользователя запрашивать API
// с его куки и читать ответы. Это та же дыра, что и без CORS, только с официальной печатью.
// Цель: Origin отражается только если он точно входит в белый список; для остальных CORS-заголовков нет вообще. `Vary: Origin` всегда, чтобы
// кэши не раздавали ответ одного сайта другому. Preflight (`OPTIONS`) разрешает только перечисленные методы и заголовки.


fn main() {}

#[derive(Debug, PartialEq, Eq)]
pub struct Forbidden;

type Headers = Vec<(String, String)>;

fn h(name: &str, value: &str) -> (String, String) {
    (name.to_string(), value.to_string())
}

const METHODS: [&str; 3] = ["GET", "POST", "DELETE"];
const REQUEST_HEADERS: [&str; 3] = ["content-type", "authorization", "x-requested-with"];

/// Заголовки CORS для обычного ответа.
pub fn cors_headers(origin: Option<&str>, allowed: &[&str]) -> Headers {
    let mut out = Headers::new();
    if let Some(origin) = origin {
        out.push(h("Access-Control-Allow-Origin", origin));
        out.push(h("Access-Control-Allow-Credentials", "true"));
    }
    out
}

/// Ответ на preflight: `method` из `Access-Control-Request-Method`, `request_headers` из `Access-Control-Request-Headers` (через запятую).
pub fn preflight(origin: Option<&str>, method: Option<&str>, request_headers: Option<&str>, allowed: &[&str]) -> Result<Headers, Forbidden> {
    let _ = (METHODS, REQUEST_HEADERS, method, request_headers, allowed);
    let origin = origin.ok_or(Forbidden)?;
    Ok(vec![
        h("Access-Control-Allow-Origin", origin),
        h("Access-Control-Allow-Credentials", "true"),
        h("Access-Control-Allow-Methods", "*"),
        h("Access-Control-Allow-Headers", "*"),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALLOWED: [&str; 2] = ["https://app.example.com", "http://localhost:3000"];

    fn get<'a>(headers: &'a Headers, name: &str) -> Option<&'a str> {
        headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    #[test]
    fn an_allowed_origin_is_echoed_with_credentials() {
        let r = cors_headers(Some("https://app.example.com"), &ALLOWED);
        assert_eq!(get(&r, "Access-Control-Allow-Origin"), Some("https://app.example.com"));
        assert_eq!(get(&r, "Access-Control-Allow-Credentials"), Some("true"));
    }

    #[test]
    fn a_foreign_origin_gets_no_cors_headers_at_all() {
        for evil in ["https://evil.com", "https://app.example.com.evil.com", "null", "http://app.example.com", ""] {
            let r = cors_headers(Some(evil), &ALLOWED);
            assert!(get(&r, "Access-Control-Allow-Origin").is_none(), "чужой Origin {evil:?} получил разрешение");
            assert!(get(&r, "Access-Control-Allow-Credentials").is_none(), "и credentials тоже");
        }
        assert!(get(&cors_headers(None, &ALLOWED), "Access-Control-Allow-Origin").is_none());
    }

    #[test]
    fn the_wildcard_is_never_used_and_responses_vary_by_origin() {
        for origin in [Some("https://app.example.com"), Some("https://evil.com"), None] {
            let r = cors_headers(origin, &ALLOWED);
            assert!(r.iter().all(|(_, v)| v != "*"), "звёздочка в ответе с credentials недопустима: {r:?}");
            assert!(get(&r, "Vary").is_some_and(|v| v.contains("Origin")), "без `Vary: Origin` общий кэш отдаст ответ для одного сайта другому ({origin:?})");
        }
    }

    #[test]
    fn preflight_allows_only_listed_methods_and_headers() {
        let ok = preflight(Some("https://app.example.com"), Some("POST"), Some("Content-Type, X-Requested-With"), &ALLOWED).unwrap();
        assert_eq!(get(&ok, "Access-Control-Allow-Origin"), Some("https://app.example.com"));
        assert_eq!(get(&ok, "Access-Control-Allow-Credentials"), Some("true"));
        let methods = get(&ok, "Access-Control-Allow-Methods").unwrap();
        assert!(methods.contains("GET") && methods.contains("DELETE") && !methods.contains('*') && !methods.contains("PUT"), "разрешённые методы: {methods:?}");
        let headers = get(&ok, "Access-Control-Allow-Headers").unwrap();
        assert!(!headers.contains('*') && headers.contains("content-type"), "разрешённые заголовки: {headers:?}");
        assert!(get(&ok, "Access-Control-Max-Age").is_some(), "preflight можно кэшировать");
    }

    #[test]
    fn preflight_refuses_everything_else() {
        let origin = Some("https://app.example.com");
        assert_eq!(preflight(Some("https://evil.com"), Some("GET"), None, &ALLOWED), Err(Forbidden), "чужой сайт");
        assert_eq!(preflight(None, Some("GET"), None, &ALLOWED), Err(Forbidden), "нет Origin");
        assert_eq!(preflight(origin, Some("PUT"), None, &ALLOWED), Err(Forbidden), "метод не из списка");
        assert_eq!(preflight(origin, None, None, &ALLOWED), Err(Forbidden), "метод не указан");
        assert_eq!(preflight(origin, Some("GET"), Some("X-Admin-Override"), &ALLOWED), Err(Forbidden), "заголовок не из списка");
        assert_eq!(preflight(origin, Some("GET"), Some("content-type, x-secret"), &ALLOWED), Err(Forbidden), "один лишний заголовок портит весь запрос");
    }
}
