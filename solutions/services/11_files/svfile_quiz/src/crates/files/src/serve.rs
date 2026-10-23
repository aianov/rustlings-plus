use crate::image::sniff;

pub struct Meta {
    pub client_name: String,
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    pub fn empty(status: u16) -> Response {
        Response { status, headers: vec![("X-Content-Type-Options".into(), "nosniff".into())], body: Vec::new() }
    }
}

fn ascii_name(name: &str) -> String {
    let cleaned: String = name.chars().filter(|c| !c.is_control()).map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') { c } else { '_' }).collect();
    if cleaned.trim().is_empty() { "file".to_string() } else { cleaned }
}

fn percent_encode(name: &str) -> String {
    let visible: String = name.chars().filter(|c| !c.is_control()).collect();
    let mut out = String::new();
    for byte in visible.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub fn serve(meta: &Meta, content: &[u8]) -> Response {
    let (content_type, mode) = match sniff(content) {
        Some(kind) => (kind.mime(), "inline"),
        None => ("application/octet-stream", "attachment"),
    };
    let disposition = format!("{mode}; filename=\"{}\"; filename*=UTF-8''{}", ascii_name(&meta.client_name), percent_encode(&meta.client_name));
    Response {
        status: 200,
        headers: vec![
            ("Content-Type".into(), content_type.into()),
            ("Content-Disposition".into(), disposition),
            ("X-Content-Type-Options".into(), "nosniff".into()),
            ("Content-Security-Policy".into(), "default-src 'none'; sandbox".into()),
            ("Content-Length".into(), content.len().to_string()),
        ],
        body: content.to_vec(),
    }
}
