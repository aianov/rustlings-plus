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
        todo!()
    }
}
