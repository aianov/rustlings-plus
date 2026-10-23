#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SameSite {
    Strict,
    Lax,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CookieError {
    BadName,
    BadValue,
    BadAttribute,
    NoneNeedsSecure,
    PrefixRules,
}

#[derive(Debug, Clone, PartialEq)]
struct SetCookie {
    name: String,
    value: String,
    max_age: Option<i64>,
    path: Option<String>,
    domain: Option<String>,
    http_only: bool,
    secure: bool,
    same_site: Option<SameSite>,
}

impl SetCookie {
    fn new(name: &str, value: &str) -> SetCookie {
        SetCookie { name: name.to_string(), value: value.to_string(), max_age: None, path: None, domain: None, http_only: false, secure: false, same_site: None }
    }

    fn delete(name: &str, path: &str) -> SetCookie {
        SetCookie { max_age: Some(0), path: Some(path.to_string()), ..SetCookie::new(name, "") }
    }

    fn to_header(&self) -> Result<String, CookieError> {
        let token = |c: u8| c.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&c);
        if self.name.is_empty() || !self.name.bytes().all(token) {
            return Err(CookieError::BadName);
        }
        let octet = |c: u8| matches!(c, 0x21 | 0x23..=0x2B | 0x2D..=0x3A | 0x3C..=0x5B | 0x5D..=0x7E);
        if !self.value.bytes().all(octet) {
            return Err(CookieError::BadValue);
        }
        let attribute = |s: &String| !s.is_empty() && s.bytes().all(|c| c >= 0x20 && c != 0x7F && c != b';');
        if self.path.as_ref().is_some_and(|p| !attribute(p)) || self.domain.as_ref().is_some_and(|d| !attribute(d)) {
            return Err(CookieError::BadAttribute);
        }
        if self.same_site == Some(SameSite::None) && !self.secure {
            return Err(CookieError::NoneNeedsSecure);
        }
        if self.name.starts_with("__Host-") && !(self.secure && self.path.as_deref() == Some("/") && self.domain.is_none()) {
            return Err(CookieError::PrefixRules);
        }
        if self.name.starts_with("__Secure-") && !self.secure {
            return Err(CookieError::PrefixRules);
        }
        let mut out = format!("{}={}", self.name, self.value);
        if let Some(age) = self.max_age {
            out.push_str(&format!("; Max-Age={age}"));
        }
        if let Some(path) = &self.path {
            out.push_str(&format!("; Path={path}"));
        }
        if let Some(domain) = &self.domain {
            out.push_str(&format!("; Domain={domain}"));
        }
        if self.http_only {
            out.push_str("; HttpOnly");
        }
        if self.secure {
            out.push_str("; Secure");
        }
        match self.same_site {
            Some(SameSite::Strict) => out.push_str("; SameSite=Strict"),
            Some(SameSite::Lax) => out.push_str("; SameSite=Lax"),
            Some(SameSite::None) => out.push_str("; SameSite=None"),
            None => {}
        }
        Ok(out)
    }
}

fn main() {
    let cookie = SetCookie { http_only: true, same_site: Some(SameSite::Lax), path: Some("/".into()), ..SetCookie::new("sid", "abc") };
    println!("{:?}", cookie.to_header());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> SetCookie {
        SetCookie {
            max_age: Some(3600),
            path: Some("/api".into()),
            domain: Some("example.com".into()),
            http_only: true,
            secure: true,
            same_site: Some(SameSite::Strict),
            ..SetCookie::new("sid", "abc123")
        }
    }

    #[test]
    fn bare_cookie_has_no_attributes() {
        assert_eq!(SetCookie::new("theme", "dark").to_header().unwrap(), "theme=dark");
    }

    #[test]
    fn all_attributes_in_a_fixed_order() {
        assert_eq!(full().to_header().unwrap(), "sid=abc123; Max-Age=3600; Path=/api; Domain=example.com; HttpOnly; Secure; SameSite=Strict");
    }

    #[test]
    fn each_attribute_alone() {
        let c = |f: fn(&mut SetCookie)| {
            let mut c = SetCookie::new("a", "1");
            f(&mut c);
            c.to_header().unwrap()
        };
        assert_eq!(c(|c| c.max_age = Some(60)), "a=1; Max-Age=60");
        assert_eq!(c(|c| c.max_age = Some(-1)), "a=1; Max-Age=-1", "отрицательный срок это «уже истёк»");
        assert_eq!(c(|c| c.path = Some("/".into())), "a=1; Path=/");
        assert_eq!(c(|c| c.http_only = true), "a=1; HttpOnly");
        assert_eq!(c(|c| c.secure = true), "a=1; Secure");
        assert_eq!(c(|c| c.same_site = Some(SameSite::Lax)), "a=1; SameSite=Lax");
    }

    #[test]
    fn deletion_must_repeat_the_path() {
        assert_eq!(SetCookie::delete("sid", "/api").to_header().unwrap(), "sid=; Max-Age=0; Path=/api", "браузер удаляет cookie с тем же именем и путём");
    }

    #[test]
    fn values_that_would_break_the_header_are_refused() {
        for bad in ["a b", "a;b", "a,b", "a\"b", "a\\b", "line\r\nSet-Cookie: x=1", "tab\there", "привет"] {
            let err = SetCookie::new("sid", bad).to_header().unwrap_err();
            assert_eq!(err, CookieError::BadValue, "значение {bad:?} нельзя класть в cookie как есть");
        }
        assert!(SetCookie::new("sid", "eyJ0.eyJ1==").to_header().is_ok(), "токены в base64 допустимы");
    }

    #[test]
    fn names_are_tokens() {
        for bad in ["", "a b", "a=b", "a;b", "имя", "a\r\nb"] {
            assert_eq!(SetCookie::new(bad, "1").to_header().unwrap_err(), CookieError::BadName, "имя {bad:?}");
        }
    }

    #[test]
    fn attributes_cannot_smuggle_more_attributes() {
        let mut c = SetCookie::new("a", "1");
        c.path = Some("/; Domain=evil.example".into());
        assert_eq!(c.to_header().unwrap_err(), CookieError::BadAttribute, "точка с запятой в пути добавила бы чужой атрибут");
        let mut c = SetCookie::new("a", "1");
        c.domain = Some("x\r\ny".into());
        assert_eq!(c.to_header().unwrap_err(), CookieError::BadAttribute);
        let mut c = SetCookie::new("a", "1");
        c.path = Some(String::new());
        assert_eq!(c.to_header().unwrap_err(), CookieError::BadAttribute, "пустой Path бессмысленен");
    }

    #[test]
    fn samesite_none_needs_secure() {
        let mut c = SetCookie::new("a", "1");
        c.same_site = Some(SameSite::None);
        assert_eq!(c.to_header().unwrap_err(), CookieError::NoneNeedsSecure, "браузеры выбросят cookie с SameSite=None без Secure");
        c.secure = true;
        assert_eq!(c.to_header().unwrap(), "a=1; Secure; SameSite=None");
    }

    #[test]
    fn name_prefixes_carry_promises() {
        let mut c = SetCookie::new("__Host-sid", "1");
        assert_eq!(c.to_header().unwrap_err(), CookieError::PrefixRules, "__Host- требует Secure и Path=/");
        c.secure = true;
        c.path = Some("/".into());
        assert_eq!(c.to_header().unwrap(), "__Host-sid=1; Path=/; Secure");
        c.domain = Some("example.com".into());
        assert_eq!(c.to_header().unwrap_err(), CookieError::PrefixRules, "__Host- не допускает Domain");
        c.domain = None;
        c.path = Some("/app".into());
        assert_eq!(c.to_header().unwrap_err(), CookieError::PrefixRules, "__Host- только с Path=/");
        let mut s = SetCookie::new("__Secure-sid", "1");
        assert_eq!(s.to_header().unwrap_err(), CookieError::PrefixRules);
        s.secure = true;
        assert!(s.to_header().is_ok());
    }
}
