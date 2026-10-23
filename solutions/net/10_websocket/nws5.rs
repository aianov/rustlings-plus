// Чат на WebSocket «не подключается в Firefox», а у коллег в Chrome всё хорошо. В журнале сервера на каждую
// попытку одна строка: «не похоже на апгрейд». А ещё за прокси компании подключение иногда пропадает, хотя у
// заголовков вроде бы правильные значения.
// TODO: Найди, почему is_ws_upgrade отвергает честные запросы, и почини, не начав при этом принимать чужие.

#![allow(dead_code)]
#[derive(Debug, Clone)]
struct Req {
    method: String,
    headers: Vec<(String, String)>,
}

impl Req {
    fn get() -> Req {
        Req { method: "GET".to_string(), headers: Vec::new() }
    }

    fn with(mut self, name: &str, value: &str) -> Req {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    fn all(&self, name: &str) -> Vec<&str> {
        self.headers.iter().filter(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str()).collect()
    }
}

fn has_token(req: &Req, name: &str, token: &str) -> bool {
    req.all(name).iter().flat_map(|v| v.split(',')).any(|t| t.trim().eq_ignore_ascii_case(token))
}

fn is_ws_upgrade(req: &Req) -> bool {
    req.method == "GET" && has_token(req, "Upgrade", "websocket") && has_token(req, "Connection", "Upgrade")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(upgrade: &str, connection: &[&str]) -> bool {
        let mut req = Req::get().with("Upgrade", upgrade);
        for line in connection {
            req = req.with("Connection", line);
        }
        is_ws_upgrade(&req)
    }

    #[test]
    fn chrome_and_safari() {
        assert!(check("websocket", &["Upgrade"]));
    }

    #[test]
    fn firefox_lists_several_tokens() {
        assert!(check("websocket", &["keep-alive, Upgrade"]), "Firefox шлёт `Connection: keep-alive, Upgrade`: Connection это список токенов");
        assert!(check("websocket", &["Upgrade, keep-alive"]), "порядок токенов не важен");
        assert!(check("websocket", &["keep-alive ,  Upgrade "]), "пробелы вокруг токенов не важны");
    }

    #[test]
    fn case_does_not_matter() {
        assert!(check("WebSocket", &["upgrade"]), "значения токенов не зависят от регистра");
        assert!(check("websocket", &["UPGRADE"]));
    }

    #[test]
    fn proxies_split_connection_into_several_lines() {
        assert!(check("websocket", &["keep-alive", "Upgrade"]), "прокси может разнести токены по нескольким строкам Connection");
    }

    #[test]
    fn ordinary_requests_are_not_upgrades() {
        assert!(!check("websocket", &["keep-alive"]), "без токена Upgrade в Connection прокси срежет Upgrade");
        assert!(!check("websocket", &["close"]));
        assert!(!check("websocket", &[]));
        assert!(!check("h2c", &["Upgrade"]), "апгрейд на другой протокол");
        assert!(!check("websocket2", &["Upgrade"]), "токен целиком, а не подстрока");
        assert!(!check("websocket", &["not-upgrade"]), "токен целиком, а не подстрока");
        assert!(!check("websocket", &["upgraded"]), "токен целиком, а не подстрока");
    }

    #[test]
    fn method_and_missing_upgrade() {
        let post = Req { method: "POST".to_string(), headers: vec![] }.with("Upgrade", "websocket").with("Connection", "Upgrade");
        assert!(!is_ws_upgrade(&post));
        let only_connection = Req::get().with("Connection", "Upgrade");
        assert!(!is_ws_upgrade(&only_connection), "без заголовка Upgrade тоже нет");
    }
}
