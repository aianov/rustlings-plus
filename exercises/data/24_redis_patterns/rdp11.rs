// «Выйти на всех устройствах»: пользователь меняет пароль, и все его сессии должны умереть сразу. Искать их
// по маске через SCAN долго и ненадёжно. Держи рядом множество токенов пользователя `<ns>:user:<id>:sessions`:
// вход добавляет токен, выход везде удаляет все сессии из множества и само множество.
// TODO: `login` (создаёт сессию на 30 минут и записывает токен во множество одним обменом), `user_of`
// (пользователь по токену или None), `logout_all`.

use course_support::redis::{Client, Error};
use uuid::Uuid;

async fn login(c: &Client, ns: &str, user: i64) -> Result<String, Error> {
    let _ = (c, ns, user, Uuid::new_v4());
    todo!()
}

async fn user_of(c: &Client, ns: &str, token: &str) -> Result<Option<i64>, Error> {
    let _ = (c, ns, token);
    todo!()
}

async fn logout_all(c: &Client, ns: &str, user: i64) -> Result<usize, Error> {
    let _ = (c, ns, user);
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn every_device_is_logged_out() {
        let s = Scope::new().await;
        let c = s.client();
        let before = c.round_trips();
        let phone = login(&c, s.prefix(), 7).await.expect("телефон");
        assert_eq!(c.round_trips() - before, 1, "вход это один обмен");
        let laptop = login(&c, s.prefix(), 7).await.expect("ноутбук");
        let tablet = login(&c, s.prefix(), 7).await.expect("планшет");
        let other = login(&c, s.prefix(), 9).await.expect("другой пользователь");
        assert_eq!(user_of(&c, s.prefix(), &laptop).await.expect("сессия"), Some(7));
        assert_eq!(logout_all(&c, s.prefix(), 7).await.expect("выход везде"), 3, "закрыто три сессии");
        for t in [&phone, &laptop, &tablet] {
            assert_eq!(user_of(&c, s.prefix(), t).await.expect("сессия"), None, "сессия {t} закрыта");
        }
        assert_eq!(user_of(&c, s.prefix(), &other).await.expect("сессия"), Some(9), "чужие сессии живы");
        assert!(s.cmd(&["EXISTS", &s.key("user:7:sessions")]).await.expect("EXISTS").int() == Some(0), "множество сессий удалено");
    }
}
