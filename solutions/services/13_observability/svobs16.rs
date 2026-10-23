
fn main() {}

pub struct Req {
    pub method: String,
    pub target: String,
    pub headers: Vec<(String, String)>,
}

const SECRET_PARAMS: [&str; 9] = ["token", "access_token", "refresh_token", "id_token", "password", "secret", "api_key", "signature", "sig"];
const SECRET_HEADERS: [&str; 5] = ["authorization", "proxy-authorization", "cookie", "set-cookie", "x-api-key"];

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() && bytes[i + 1].is_ascii_hexdigit() && bytes[i + 2].is_ascii_hexdigit() {
            let hex = &text[i + 1..i + 3];
            out.push(u8::from_str_radix(hex, 16).unwrap_or(b'%'));
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

pub fn access_line(req: &Req, status: u16) -> String {
    let mut line = format!("{} {} {}", printable(&req.method), redact_target(&req.target), status);
    for (name, value) in &req.headers {
        let name = name.to_ascii_lowercase();
        let shown = if SECRET_HEADERS.contains(&name.as_str()) { "[redacted]".to_string() } else { printable(value) };
        line.push_str(&format!(" {}=\"{}\"", printable(&name), shown));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(target: &str, headers: &[(&str, &str)]) -> Req {
        Req { method: "GET".to_string(), target: target.to_string(), headers: headers.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect() }
    }

    #[test]
    fn an_ordinary_request_is_logged_as_is() {
        let line = access_line(&req("/feed?page=2&sort=new", &[("User-Agent", "curl/8.0"), ("Accept", "*/*")]), 200);
        assert_eq!(line, "GET /feed?page=2&sort=new 200 user-agent=\"curl/8.0\" accept=\"*/*\"", "обычный запрос не меняется (кроме регистра имён заголовков)");
    }

    #[test]
    fn secret_query_parameters_are_hidden_but_their_presence_is_visible() {
        let line = access_line(&req("/export?page=2&token=abc123&format=csv&API_KEY=k-777", &[]), 200);
        assert!(!line.contains("abc123") && !line.contains("k-777"), "секрет попал в лог: {line}");
        assert!(line.contains("token=[redacted]") && line.contains("API_KEY=[redacted]"), "факт наличия параметра виден: {line}");
        assert!(line.contains("page=2") && line.contains("format=csv"), "остальные параметры остаются: {line}");
    }

    #[test]
    fn every_known_secret_name_is_covered() {
        for name in ["token", "access_token", "refresh_token", "id_token", "password", "secret", "api_key", "signature", "sig"] {
            let line = access_line(&req(&format!("/x?{name}=TOPSECRET"), &[]), 200);
            assert!(!line.contains("TOPSECRET"), "параметр {name} не скрыт: {line}");
        }
    }

    #[test]
    fn an_encoded_parameter_name_does_not_get_around_the_filter() {
        let line = access_line(&req("/x?%74oken=TOPSECRET&Access%5FToken=ALSOSECRET", &[]), 200);
        assert!(!line.contains("TOPSECRET") && !line.contains("ALSOSECRET"), "имя в процентной записи обошло фильтр: {line}");
    }

    #[test]
    fn a_secret_word_inside_a_value_does_not_hide_the_rest() {
        let line = access_line(&req("/search?q=my+token=x&notoken=1&token_count=5", &[]), 200);
        assert!(line.contains("q=my+token=x") && line.contains("notoken=1") && line.contains("token_count=5"), "скрываются параметры с именем-секретом, а не любые со словом внутри: {line}");
    }

    #[test]
    fn secret_headers_are_hidden_whatever_their_case() {
        let line = access_line(&req("/me", &[("AUTHORIZATION", "Bearer eyJhbGciOi"), ("Cookie", "session=SESSIONVALUE"), ("x-api-key", "KEYVALUE"), ("Set-Cookie", "a=b"), ("Accept", "application/json")]), 200);
        for secret in ["eyJhbGciOi", "SESSIONVALUE", "KEYVALUE", "a=b"] {
            assert!(!line.contains(secret), "секрет {secret} попал в лог: {line}");
        }
        assert!(line.contains("authorization=\"[redacted]\"") && line.contains("cookie=\"[redacted]\""), "имя заголовка остаётся, значение скрыто: {line}");
        assert!(line.contains("accept=\"application/json\""));
    }

    #[test]
    fn client_data_cannot_break_the_line_or_forge_another_one() {
        let evil = Req { method: "GET".to_string(), target: "/a\nINFO fake line".to_string(), headers: vec![("User-Agent".to_string(), "x\r\n200 forged \"entry\"\u{7}".to_string())] };
        let line = access_line(&evil, 200);
        assert!(!line.contains('\n') && !line.contains('\r'), "перевод строки из данных клиента разорвал запись: {line:?}");
        assert!(line.chars().all(|c| !c.is_control()), "управляющие символы остались в строке: {line:?}");
        assert_eq!(line.matches('"').count() - line.matches("\\\"").count(), 2, "кавычка в значении не должна закрывать значение раньше времени: {line}");
    }

    #[test]
    fn a_parameter_without_a_value_and_odd_queries_do_not_panic() {
        for target in ["/x?token", "/x?", "/x?&&=&", "/x?token=", "/x?%", "/x?%7"] {
            let line = access_line(&req(target, &[]), 400);
            assert!(line.starts_with("GET /x"), "{target}: {line}");
        }
    }
}
