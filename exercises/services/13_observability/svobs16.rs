// Access-лог пишет по строке на запрос: метод, адрес, код и заголовки. Логи пересылаются в общий агрегатор, их читают
// десятки людей и сторонних сервисов, и вчера в них обнаружились чужие сессионные токены: ссылка вида `?token=...` и
// заголовок `Authorization` печатались как есть. Заодно в строку прорвался перевод строки из заголовка, и под видом
// запроса появилась поддельная запись. Цель: секреты в лог не попадают, а факт их наличия виден (`token=[redacted]`);
// данные клиента не могут разорвать строку. Формат строки: `МЕТОД адрес код имя="значение" ...`, имена заголовков в
// нижнем регистре, значения в кавычках.


fn main() {}

pub struct Req {
    pub method: String,
    pub target: String,
    pub headers: Vec<(String, String)>,
}

pub fn access_line(req: &Req, status: u16) -> String {
    let mut line = format!("{} {} {}", req.method, req.target, status);
    for (name, value) in &req.headers {
        line.push_str(&format!(" {name}=\"{value}\""));
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
