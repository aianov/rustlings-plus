// Ник пользователя попадает в заголовок ответа: `X-Nick: <ник>`. Кто-то зарегистрировал ник, содержащий перевод строки и дальше
// `Set-Cookie: session=чужой`. Сервер аккуратно подставил его в ответ, и один ответ превратился в два заголовка (а при желании и в два
// ответа): атакующий ставит жертве куки, подменяет страницу, отравляет кэш. Это внедрение в заголовки (response splitting).
// Цель: `build_response` никогда не выпускает значение, из которого получаются лишние строки. Недопустимые имя или значение дают
// `Err(BadHeader)`, а не молчаливую «починку». Нормальные значения, в том числе с кириллицей, проходят.


fn main() {}

#[derive(Debug, PartialEq, Eq)]
pub struct BadHeader;

pub fn build_response(status: u16, headers: &[(&str, &str)], body: &str) -> Result<String, BadHeader> {
    let mut out = format!("HTTP/1.1 {status} OK\r\n");
    for (name, value) in headers {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Сколько строк заголовков видит разбор: каждая строка заканчивается CRLF, пустая строка завершает заголовки.
    fn header_lines(raw: &str) -> Vec<&str> {
        raw.split("\r\n\r\n").next().unwrap().split("\r\n").skip(1).collect()
    }

    #[test]
    fn normal_headers_pass() {
        let raw = build_response(200, &[("X-Nick", "Аня"), ("Cache-Control", "no-store")], "тело").unwrap();
        let lines = header_lines(&raw);
        assert_eq!(lines.len(), 3, "два наших заголовка и Content-Length: {lines:?}");
        assert!(lines.contains(&"X-Nick: Аня"));
        assert!(raw.ends_with("\r\n\r\nтело"));
    }

    #[test]
    fn line_breaks_in_values_are_refused() {
        for bad in ["a\r\nSet-Cookie: session=evil", "a\nSet-Cookie: x=1", "a\rX: y", "a\0b", "a\u{85}b", "конец\n"] {
            let r = build_response(200, &[("X-Nick", bad)], "");
            assert_eq!(r, Err(BadHeader), "значение {bad:?} прошло: из него получаются лишние заголовки (даже одинокий \\n или \\r многие серверы считают концом строки)");
        }
    }

    #[test]
    fn bad_names_are_refused() {
        for bad in ["", "X Nick", "X-Nick:", "X\r\nEvil", "Имя", "X-Ник"] {
            assert_eq!(build_response(200, &[(bad, "v")], ""), Err(BadHeader), "имя заголовка {bad:?} прошло");
        }
    }

    #[test]
    fn a_tab_inside_a_value_is_fine() {
        assert!(build_response(200, &[("X-Note", "a\tb")], "").is_ok());
    }
}
