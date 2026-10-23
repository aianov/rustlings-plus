#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieError {
    BadName,
    BadValue,
    BadAttribute,
    NoneNeedsSecure,
    PrefixRules,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetCookie {
    pub name: String,
    pub value: String,
    pub max_age: Option<i64>,
    pub path: Option<String>,
    pub domain: Option<String>,
    pub http_only: bool,
    pub secure: bool,
    pub same_site: Option<SameSite>,
}

pub fn parse_cookies(header: &str) -> Vec<(String, String)> {
    header
        .split(';')
        .filter_map(|pair| {
            let (name, value) = pair.split_once('=')?;
            let name = name.trim();
            if name.is_empty() || name.bytes().any(|b| b.is_ascii_whitespace() || b.is_ascii_control()) {
                return None;
            }
            let value = value.trim();
            let value = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(value);
            Some((name.to_string(), value.to_string()))
        })
        .collect()
}

pub fn cookie<'a>(pairs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    pairs.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
}

impl SetCookie {
    pub fn new(name: &str, value: &str) -> SetCookie {
        SetCookie { name: name.to_string(), value: value.to_string(), max_age: None, path: None, domain: None, http_only: false, secure: false, same_site: None }
    }

    pub fn delete(name: &str, path: &str) -> SetCookie {
        SetCookie { max_age: Some(0), path: Some(path.to_string()), ..SetCookie::new(name, "") }
    }

    pub fn to_header(&self) -> Result<String, CookieError> {
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
