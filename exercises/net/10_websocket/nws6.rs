// Браузер при подключении по WebSocket не применяет к странице правила CORS: любая страница, на которую зашёл
// пользователь, может открыть сокет на наш сервер, и браузер приложит его cookie. Единственная защита: сервер сам
// смотрит на заголовок Origin и решает, чьим страницам доверяет. Клиенты не из браузера (утилиты, тесты) Origin обычно
// не присылают, и это допустимо, если сервис так настроен.
// TODO: Допиши check_origin. Сравнивай Origin целиком с разрешёнными, без учёта регистра. Отказ это (403, причина).

#![allow(dead_code)]


struct OriginPolicy {
    allowed: Vec<String>,
    allow_missing: bool,
}

fn check_origin(policy: &OriginPolicy, origin: Option<&str>) -> Result<(), (u16, &'static str)> {
    let _ = (policy, origin);
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(allow_missing: bool) -> OriginPolicy {
        OriginPolicy { allowed: vec!["https://app.example".to_string(), "http://localhost:3000".to_string()], allow_missing }
    }

    #[test]
    fn listed_origins_pass() {
        let p = policy(false);
        assert_eq!(check_origin(&p, Some("https://app.example")), Ok(()));
        assert_eq!(check_origin(&p, Some("http://localhost:3000")), Ok(()));
        assert_eq!(check_origin(&p, Some("HTTPS://APP.EXAMPLE")), Ok(()), "хост и схема не зависят от регистра");
    }

    #[test]
    fn similar_looking_origins_are_strangers() {
        let p = policy(false);
        for origin in [
            "https://app.example.evil.com",
            "https://evilapp.example",
            "https://sub.app.example",
            "http://app.example",
            "https://app.example:8443",
            "https://app.example/",
            "https://app.example/path",
            "http://localhost:3001",
            "http://localhost",
            "",
            " https://app.example",
        ] {
            let result = check_origin(&p, Some(origin));
            assert!(matches!(result, Err((403, _))), "Origin {origin:?} не из списка, а пропущен: сравнивай целиком, не по префиксу и не по подстроке");
        }
    }

    #[test]
    fn opaque_origin_is_never_trusted() {
        let mut p = policy(true);
        p.allowed.push("null".to_string());
        assert!(matches!(check_origin(&p, Some("null")), Err((403, _))), "`null` шлют песочницы и локальные файлы, доверять ему нельзя даже из списка");
    }

    #[test]
    fn missing_origin_depends_on_policy() {
        assert_eq!(check_origin(&policy(true), None), Ok(()), "утилита без Origin разрешена настройкой");
        assert!(matches!(check_origin(&policy(false), None), Err((403, _))), "строгая настройка требует Origin");
    }

    #[test]
    fn empty_list_allows_nobody() {
        let p = OriginPolicy { allowed: vec![], allow_missing: false };
        assert!(check_origin(&p, Some("https://app.example")).is_err());
    }
}
