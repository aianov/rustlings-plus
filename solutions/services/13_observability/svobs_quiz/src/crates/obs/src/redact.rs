const SECRET_PARAMS: [&str; 9] = ["token", "access_token", "refresh_token", "id_token", "password", "secret", "api_key", "signature", "sig"];
const SECRET_HEADERS: [&str; 5] = ["authorization", "proxy-authorization", "cookie", "set-cookie", "x-api-key"];

pub struct Req<'a> {
    pub method: &'a str,
    pub target: &'a str,
    pub headers: &'a [(String, String)],
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() && bytes[i + 1].is_ascii_hexdigit() && bytes[i + 2].is_ascii_hexdigit() {
            out.push(u8::from_str_radix(&text[i + 1..i + 3], 16).unwrap_or(b'%'));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn printable(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn redact_target(target: &str) -> String {
    let Some((path, query)) = target.split_once('?') else {
        return printable(target);
    };
    let cleaned: Vec<String> = query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((name, _)) if SECRET_PARAMS.contains(&percent_decode(name).to_ascii_lowercase().as_str()) => format!("{}=[redacted]", printable(name)),
            _ => printable(pair),
        })
        .collect();
    format!("{}?{}", printable(path), cleaned.join("&"))
}

pub fn access_line(req: &Req<'_>, status: u16) -> String {
    let mut line = format!("{} {} {}", printable(req.method), redact_target(req.target), status);
    for (name, value) in req.headers {
        let name = name.to_ascii_lowercase();
        let shown = if SECRET_HEADERS.contains(&name.as_str()) { "[redacted]".to_string() } else { printable(value) };
        line.push_str(&format!(" {}=\"{}\"", printable(&name), shown));
    }
    line
}
