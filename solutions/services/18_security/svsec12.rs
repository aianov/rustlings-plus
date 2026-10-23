
use std::collections::{HashMap, HashSet};

fn main() {}

#[derive(Debug, PartialEq, Eq)]
pub enum Reject {
    MethodNotAllowed,
    NoSession,
    Csrf,
}

pub struct Req {
    pub method: String,
    pub cookies: HashMap<String, String>,
    /// Заголовки в нижнем регистре.
    pub headers: HashMap<String, String>,
}

const APP_ORIGIN: &str = "https://app.example.com";

/// Значение `Set-Cookie` для refresh-токена.
pub fn refresh_cookie(token: &str) -> String {
    format!("refresh={token}; Max-Age=2592000; Path=/auth/refresh; HttpOnly; Secure; SameSite=Strict")
}

/// Проверяет запрос на обновление и возвращает токен из куки.
pub fn refresh(req: &Req, valid: &HashSet<String>) -> Result<String, Reject> {
    if req.method != "POST" {
        return Err(Reject::MethodNotAllowed);
    }
    let token = req.cookies.get("refresh").filter(|t| valid.contains(*t)).ok_or(Reject::NoSession)?;
    if req.headers.get("x-requested-with").map(String::as_str) != Some("fetch") {
        return Err(Reject::Csrf);
    }
    if req.headers.get("origin").is_some_and(|o| o != APP_ORIGIN) {
        return Err(Reject::Csrf);
    }
    if req.headers.get("sec-fetch-site").is_some_and(|s| s != "same-origin" && s != "same-site") {
        return Err(Reject::Csrf);
    }
    let _ = APP_ORIGIN;
    Ok(token.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> HashSet<String> {
        HashSet::from(["good-token".to_string()])
    }

    fn req(method: &str, cookie: Option<&str>, headers: &[(&str, &str)]) -> Req {
        Req {
            method: method.to_string(),
            cookies: cookie.map(|c| HashMap::from([("refresh".to_string(), c.to_string())])).unwrap_or_default(),
            headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }

    #[test]
    fn our_own_frontend_works() {
        let r = req("POST", Some("good-token"), &[("x-requested-with", "fetch"), ("origin", "https://app.example.com"), ("sec-fetch-site", "same-origin")]);
        assert_eq!(refresh(&r, &valid()), Ok("good-token".to_string()));
        let old_browser = req("POST", Some("good-token"), &[("x-requested-with", "fetch")]);
        assert_eq!(refresh(&old_browser, &valid()), Ok("good-token".to_string()), "старый браузер без Sec-Fetch-Site и без Origin, но со своим заголовком");
    }

    #[test]
    fn a_form_on_a_foreign_site_is_refused() {
        let form = req("POST", Some("good-token"), &[("origin", "https://evil.example"), ("sec-fetch-site", "cross-site"), ("content-type", "application/x-www-form-urlencoded")]);
        assert_eq!(refresh(&form, &valid()), Err(Reject::Csrf), "форма с чужого сайта обновила сессию");
        let plain = req("POST", Some("good-token"), &[]);
        assert_eq!(refresh(&plain, &valid()), Err(Reject::Csrf), "без нашего заголовка запрос могла отправить любая страница");
    }

    #[test]
    fn the_custom_header_alone_is_not_enough_when_the_browser_says_cross_site() {
        let r = req("POST", Some("good-token"), &[("x-requested-with", "fetch"), ("sec-fetch-site", "cross-site")]);
        assert_eq!(refresh(&r, &valid()), Err(Reject::Csrf));
        let r = req("POST", Some("good-token"), &[("x-requested-with", "fetch"), ("origin", "https://app.example.com.evil.com")]);
        assert_eq!(refresh(&r, &valid()), Err(Reject::Csrf), "Origin обязан совпадать точно");
        let r = req("POST", Some("good-token"), &[("x-requested-with", "XMLHttpRequest")]);
        assert_eq!(refresh(&r, &valid()), Err(Reject::Csrf), "значение заголовка тоже наше");
    }

    #[test]
    fn method_and_session_are_checked() {
        let headers = [("x-requested-with", "fetch")];
        assert_eq!(refresh(&req("GET", Some("good-token"), &headers), &valid()), Err(Reject::MethodNotAllowed));
        assert_eq!(refresh(&req("POST", None, &headers), &valid()), Err(Reject::NoSession));
        assert_eq!(refresh(&req("POST", Some("stolen"), &headers), &valid()), Err(Reject::NoSession));
    }

    #[test]
    fn the_cookie_has_the_protective_attributes() {
        let c = refresh_cookie("abc");
        let lower = c.to_ascii_lowercase();
        assert!(c.starts_with("refresh=abc;"));
        for needed in ["httponly", "secure", "samesite=strict", "path=/auth/refresh"] {
            assert!(lower.contains(needed), "в куки нет атрибута {needed}: {c}");
        }
        assert!(!lower.contains("samesite=none"));
    }
}
