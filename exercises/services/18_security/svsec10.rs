// Браузер при открытии WebSocket сам шлёт заголовок `Origin`: с какого сайта запущен скрипт. Куки пользователя к нашему адресу браузер
// прикладывает автоматически, к чьему бы скрипту ни относился запрос. Если сервер не смотрит на Origin, любой сайт в соседней вкладке открывает
// соединение «от имени» пользователя (cross-site WebSocket hijacking) и читает его личные сообщения. Сейчас проверка сводится к «если в Origin встречается
// нужная строка, пускаем», и атакующий регистрирует `app.example.com.evil.com`.
// Цель: Origin сверяется с белым списком точно (схема, имя без учёта регистра, порт; стандартный порт можно не писать). Для аутентификации по куки
// Origin обязателен. Без куки (клиент с токеном в заголовке, не браузер) отсутствие Origin допустимо, но присутствующий Origin всё равно проверяется.


fn main() {}

#[derive(Debug, PartialEq, Eq)]
pub enum Reject {
    MissingOrigin,
    ForbiddenOrigin,
}

pub struct Handshake<'a> {
    pub origin: Option<&'a str>,
    pub cookie_auth: bool,
}


pub fn check_origin(h: &Handshake, allowed: &[&str]) -> Result<(), Reject> {
    match h.origin {
        None => Ok(()),
        Some(origin) if allowed.iter().any(|a| origin.contains(a)) => Ok(()),
        Some(_) => Err(Reject::ForbiddenOrigin),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALLOWED: [&str; 2] = ["https://app.example.com", "http://localhost:3000"];

    fn cookie(origin: Option<&str>) -> Result<(), Reject> {
        check_origin(&Handshake { origin, cookie_auth: true }, &ALLOWED)
    }

    #[test]
    fn the_allowed_origins_pass() {
        for ok in ["https://app.example.com", "https://APP.example.COM", "HTTPS://app.example.com", "https://app.example.com:443", "http://localhost:3000"] {
            assert_eq!(cookie(Some(ok)), Ok(()), "{ok:?} из белого списка отвергнут");
        }
    }

    #[test]
    fn lookalikes_are_rejected() {
        for bad in [
            "https://app.example.com.evil.com",
            "https://evilapp.example.com",
            "https://app.example.com@evil.com",
            "https://evil.com/https://app.example.com",
            "http://app.example.com",
            "https://app.example.com:8443",
            "http://localhost",
            "http://localhost:3001",
            "http://localhost:30000",
            "https://app.example.com/",
            "null",
            "",
            "app.example.com",
            "ftp://app.example.com",
            "https://",
            "https://app.example.com\\.evil.com",
        ] {
            assert_eq!(cookie(Some(bad)), Err(Reject::ForbiddenOrigin), "{bad:?} прошёл, а это чужой сайт или подделка");
        }
    }

    #[test]
    fn cookie_sessions_require_an_origin() {
        assert_eq!(cookie(None), Err(Reject::MissingOrigin), "браузер всегда присылает Origin; его отсутствие при куки подозрительно");
    }

    #[test]
    fn token_clients_may_omit_it_but_not_lie() {
        let token = |origin| check_origin(&Handshake { origin, cookie_auth: false }, &ALLOWED);
        assert_eq!(token(None), Ok(()), "мобильное приложение и скрипты не шлют Origin");
        assert_eq!(token(Some("https://evil.com")), Err(Reject::ForbiddenOrigin), "если Origin есть, он проверяется всегда");
        assert_eq!(token(Some("https://app.example.com")), Ok(()));
    }
}
