// Код ответа это число, но у него есть смысл: класс (первая цифра), стандартная причина и важное
// правило про тело. Принимать «любое число» нельзя, коды вне 100..=599 в HTTP не бывают.
// Причины (имена из RFC 7231, в новых RFC кое-где другие): 100 Continue, 101 Switching Protocols, 200 OK,
// 201 Created, 202 Accepted, 204 No Content, 206 Partial Content, 301 Moved Permanently, 302 Found,
// 304 Not Modified, 307 Temporary Redirect, 308 Permanent Redirect, 400 Bad Request, 401 Unauthorized,
// 403 Forbidden, 404 Not Found, 405 Method Not Allowed, 408 Request Timeout, 411 Length Required,
// 413 Payload Too Large, 414 URI Too Long, 416 Range Not Satisfiable, 431 Request Header Fields Too Large,
// 500 Internal Server Error, 501 Not Implemented, 502 Bad Gateway, 503 Service Unavailable, 504 Gateway Timeout.
// TODO: Реализуй StatusCode: new, code, reason (для этих кодов по таблице, для остальных пустая строка),
// class и allows_body. Тела не бывает у 1xx, 204 и 304.

#[derive(Debug, PartialEq, Clone, Copy)]
struct StatusCode(u16);

#[derive(Debug, PartialEq, Clone, Copy)]
enum Class {
    Informational,
    Success,
    Redirection,
    ClientError,
    ServerError,
}

impl StatusCode {
    fn new(code: u16) -> Option<StatusCode> {
        todo!()
    }

    fn code(self) -> u16 {
        todo!()
    }

    fn reason(self) -> &'static str {
        todo!()
    }

    fn class(self) -> Class {
        todo!()
    }

    fn allows_body(self) -> bool {
        todo!()
    }
}

fn main() {
    let s = StatusCode::new(404).expect("код");
    println!("{} {} {:?}", s.code(), s.reason(), s.class());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(c: u16) -> StatusCode {
        StatusCode::new(c).unwrap_or_else(|| panic!("код {c} должен быть допустим"))
    }

    #[test]
    fn range_is_checked() {
        for bad in [0, 99, 600, 1000, 65535] {
            assert_eq!(StatusCode::new(bad), None, "{bad} не может быть кодом ответа");
        }
        assert_eq!(StatusCode::new(100).map(StatusCode::code), Some(100));
        assert_eq!(StatusCode::new(599).map(StatusCode::code), Some(599));
    }

    #[test]
    fn reasons() {
        let table = [
            (100, "Continue"), (101, "Switching Protocols"), (200, "OK"), (201, "Created"), (202, "Accepted"), (204, "No Content"),
            (206, "Partial Content"), (301, "Moved Permanently"), (302, "Found"), (304, "Not Modified"), (307, "Temporary Redirect"),
            (308, "Permanent Redirect"), (400, "Bad Request"), (401, "Unauthorized"), (403, "Forbidden"), (404, "Not Found"),
            (405, "Method Not Allowed"), (408, "Request Timeout"), (411, "Length Required"), (413, "Payload Too Large"),
            (414, "URI Too Long"), (416, "Range Not Satisfiable"), (431, "Request Header Fields Too Large"),
            (500, "Internal Server Error"), (501, "Not Implemented"), (502, "Bad Gateway"), (503, "Service Unavailable"), (504, "Gateway Timeout"),
        ];
        for (code, reason) in table {
            assert_eq!(st(code).reason(), reason, "причина для {code}");
        }
        assert_eq!(st(299).reason(), "", "для неизвестных кодов причина пустая");
        assert_eq!(st(599).reason(), "");
    }

    #[test]
    fn classes_come_from_the_first_digit() {
        assert_eq!(st(100).class(), Class::Informational);
        assert_eq!(st(226).class(), Class::Success, "неизвестный код 226 всё равно успех");
        assert_eq!(st(399).class(), Class::Redirection);
        assert_eq!(st(418).class(), Class::ClientError);
        assert_eq!(st(599).class(), Class::ServerError);
    }

    #[test]
    fn bodyless_statuses() {
        for c in [100, 101, 199, 204, 304] {
            assert!(!st(c).allows_body(), "у {c} тела быть не может");
        }
        for c in [200, 201, 205, 206, 301, 302, 404, 500] {
            assert!(st(c).allows_body(), "у {c} тело разрешено");
        }
    }
}
