// Страница входа делает редирект на адрес из параметра ?next=..., а имя пользователя кладёт в cookie.
// Пока значения безобидны, всё работает. Но если в значении есть "\r\n", оно закончит текущий
// заголовок и допишет любые чужие: Set-Cookie, пустую строку и целый поддельный ответ. Это называется
// расщеплением ответа (response splitting).
// TODO: Сделай так, чтобы чужое имя или значение не могло испортить ответ: в таком случае header
// возвращает BadHeader, и заголовок в ответ не попадает. Табуляция в значении и не-ASCII допустимы.
// redirect после этого должен стать безопасным без правок.

#[derive(Debug, PartialEq)]
enum ParseError {
    BadHeader,
}

#[derive(Debug)]
struct Response {
    status: u16,
    headers: Vec<(String, String)>,
}

impl Response {
    fn new(status: u16) -> Response {
        Response { status, headers: Vec::new() }
    }

    fn header(&mut self, name: &str, value: &str) -> Result<(), ParseError> {
        self.headers.push((name.to_string(), value.to_string()));
        Ok(())
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut out = format!("HTTP/1.1 {} \r\n", self.status).into_bytes();
        for (name, value) in &self.headers {
            out.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
        }
        out.extend_from_slice(b"Content-Length: 0\r\n\r\n");
        out
    }
}

fn redirect(location: &str) -> Result<Response, ParseError> {
    let mut response = Response::new(302);
    response.header("Location", location)?;
    Ok(response)
}

fn main() {
    match redirect("/home") {
        Ok(r) => print!("{}", String::from_utf8_lossy(&r.to_bytes())),
        Err(e) => println!("{e:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn honest_values_work() {
        let r = redirect("/home?tab=1").expect("обычный адрес должен проходить");
        assert_eq!(String::from_utf8_lossy(&r.to_bytes()), "HTTP/1.1 302 \r\nLocation: /home?tab=1\r\nContent-Length: 0\r\n\r\n");
        let mut r = Response::new(200);
        r.header("X-Note", "tab\tseparated").expect("табуляция в значении допустима");
        r.header("Set-Cookie", "name=\"Иван\"; Path=/").expect("не-ASCII в значении допустим");
        assert_eq!(r.headers.len(), 2);
    }

    #[test]
    fn crlf_in_value_is_rejected() {
        let evil = "/x\r\nSet-Cookie: admin=1";
        assert_eq!(redirect(evil).unwrap_err(), ParseError::BadHeader, "значение с CRLF дописало бы чужой заголовок");
    }

    #[test]
    fn lone_cr_or_lf_is_rejected_too() {
        for evil in ["/x\nSet-Cookie: admin=1", "/x\rSet-Cookie: admin=1", "/x\0y", "/x\x1by"] {
            assert!(redirect(evil).is_err(), "{evil:?}: часть серверов считает одинокий LF концом строки, поэтому отвергать надо и его");
        }
    }

    #[test]
    fn rejected_header_is_not_stored() {
        let mut r = Response::new(200);
        assert!(r.header("X-Ok", "1").is_ok());
        assert!(r.header("X-Bad", "1\r\nInjected: yes").is_err());
        assert_eq!(r.headers.len(), 1, "отвергнутый заголовок не должен остаться в ответе");
        assert!(!String::from_utf8_lossy(&r.to_bytes()).contains("Injected"));
    }

    #[test]
    fn bad_names_are_rejected() {
        for name in ["", "X Space", "X:Colon", "X-A\r\nEvil", "Имя", "X(Paren", "X-A\n"] {
            let mut r = Response::new(200);
            assert_eq!(r.header(name, "v"), Err(ParseError::BadHeader), "имя {name:?} недопустимо");
        }
    }
}
