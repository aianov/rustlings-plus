// Сейчас шлюз принимает любое WebSocket-подключение, а кто это, выясняет уже по первому сообщению. Анонимы успевают занять
// соединение, память и место в реестре. Проверять надо до ответа 101: на плохой токен честное 401 и никакого сокета.
// Браузер не умеет ставить заголовки при WebSocket-подключении, поэтому токен приходит в адресе (`?token=`), а мобильные клиенты
// шлют `Authorization: Bearer`. TODO: Допиши decide, который решает, пускать ли клиента. Токен проверяет трейт Authenticator.

#![allow(dead_code)]

type UserId = u64;

struct Head {
    target: String,
    headers: Vec<(String, String)>,
}

impl Head {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, PartialEq)]
enum Decision {
    Upgrade(UserId),
    Reject(u16, String),
}

#[derive(Debug)]
enum AuthError {
    Unavailable,
}

trait Authenticator {
    async fn check(&self, token: &str) -> Result<Option<UserId>, AuthError>;
}

fn token_from(head: &Head) -> Result<Option<String>, Decision> {
    if let Some(value) = head.header("authorization") {
        if let Some((scheme, rest)) = value.split_once(' ') {
            if scheme.eq_ignore_ascii_case("bearer") {
                let token = rest.trim();
                return Ok((!token.is_empty()).then(|| token.to_string()));
            }
        }
    }
    let Some((_, query)) = head.target.split_once('?') else {
        return Ok(None);
    };
    let mut found = None;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key == "token" {
            if found.is_some() {
                return Err(Decision::Reject(400, "token указан дважды".to_string()));
            }
            found = Some(value.to_string());
        }
    }
    Ok(found.filter(|t| !t.is_empty()))
}

async fn decide<A: Authenticator>(auth: &A, head: &Head) -> Decision {
    let token = match token_from(head) {
        Ok(Some(token)) => token,
        Ok(None) => return Decision::Reject(401, "нужен токен".to_string()),
        Err(decision) => return decision,
    };
    match auth.check(&token).await {
        Ok(Some(user)) => Decision::Upgrade(user),
        Ok(None) => Decision::Reject(401, "токен не подошёл".to_string()),
        Err(AuthError::Unavailable) => Decision::Reject(503, "проверка токенов недоступна".to_string()),
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct Fake {
        calls: AtomicUsize,
        down: bool,
    }

    impl Authenticator for Fake {
        async fn check(&self, token: &str) -> Result<Option<UserId>, AuthError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.down {
                return Err(AuthError::Unavailable);
            }
            Ok(match token {
                "alice-secret" => Some(1),
                "bob-secret" => Some(2),
                _ => None,
            })
        }
    }

    fn head(target: &str, headers: &[(&str, &str)]) -> Head {
        Head { target: target.to_string(), headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() }
    }

    #[tokio::test]
    async fn token_from_query_among_other_parameters() {
        let auth = Fake::default();
        assert_eq!(decide(&auth, &head("/ws?room=7&token=alice-secret&x=1", &[])).await, Decision::Upgrade(1));
        assert_eq!(decide(&auth, &head("/ws?token=bob-secret", &[])).await, Decision::Upgrade(2));
    }

    #[tokio::test]
    async fn token_from_authorization_header() {
        let auth = Fake::default();
        assert_eq!(decide(&auth, &head("/ws", &[("Authorization", "Bearer alice-secret")])).await, Decision::Upgrade(1));
        assert_eq!(decide(&auth, &head("/ws", &[("authorization", "bearer bob-secret")])).await, Decision::Upgrade(2), "имя заголовка и схема не зависят от регистра");
    }

    #[tokio::test]
    async fn header_wins_over_query_and_other_schemes_fall_back_to_query() {
        let auth = Fake::default();
        let both = head("/ws?token=bob-secret", &[("Authorization", "Bearer alice-secret")]);
        assert_eq!(decide(&auth, &both).await, Decision::Upgrade(1), "если токен есть и в заголовке, и в адресе, главный заголовок");
        let basic = head("/ws?token=bob-secret", &[("Authorization", "Basic YWxpY2U6cHc=")]);
        assert_eq!(decide(&auth, &basic).await, Decision::Upgrade(2), "чужая схема авторизации не мешает токену из адреса");
    }

    #[tokio::test]
    async fn missing_or_empty_token_is_401_and_auth_is_not_asked() {
        let auth = Fake::default();
        for h in [head("/ws", &[]), head("/ws?token=", &[]), head("/ws?token", &[]), head("/ws?mytoken=alice-secret", &[]), head("/ws", &[("Authorization", "Bearer ")])] {
            match decide(&auth, &h).await {
                Decision::Reject(401, _) => {}
                other => panic!("для {:?} ждали 401, получили {other:?}", h.target),
            }
        }
        assert_eq!(auth.calls.load(Ordering::SeqCst), 0, "без токена в сервис проверки ходить незачем");
    }

    #[tokio::test]
    async fn wrong_token_is_401_without_echoing_it() {
        let auth = Fake::default();
        match decide(&auth, &head("/ws?token=hunter2-leaked", &[])).await {
            Decision::Reject(401, text) => assert!(!text.contains("hunter2"), "токен нельзя возвращать в тексте ошибки: он попадёт в логи и на экран"),
            other => panic!("ждали 401, получили {other:?}"),
        }
    }

    #[tokio::test]
    async fn two_tokens_in_one_query_are_ambiguous() {
        let auth = Fake::default();
        match decide(&auth, &head("/ws?token=bob-secret&token=alice-secret", &[])).await {
            Decision::Reject(400, _) => {}
            other => panic!("два параметра token неоднозначны, ждали 400, получили {other:?}"),
        }
        assert_eq!(auth.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn auth_outage_is_503_not_401() {
        let auth = Fake { down: true, ..Fake::default() };
        match decide(&auth, &head("/ws?token=alice-secret", &[])).await {
            Decision::Reject(503, _) => {}
            other => panic!("сервис проверки лежит, это 503: клиент не должен решить, что токен плохой и стереть его. Получили {other:?}"),
        }
    }
}
