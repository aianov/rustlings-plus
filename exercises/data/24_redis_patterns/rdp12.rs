// Статус «в сети»: клиент шлёт heartbeat раз в 10 секунд, и ключ присутствия живёт 30 секунд. Список друзей
// показывает, кто в сети, а у пользователя бывает 300 друзей: проверка по одной команде на друга это 300
// обменов с Redis на каждое открытие списка. Почини `online_friends` так, чтобы проверка шла одним обменом.

use course_support::redis::{Client, Error};

async fn heartbeat(c: &Client, ns: &str, user: i64) -> Result<(), Error> {
    c.cmd(&["SET", &format!("{ns}:presence:{user}"), "1", "EX", "30"]).await?;
    Ok(())
}

async fn online_friends(c: &Client, ns: &str, friends: &[i64]) -> Result<Vec<i64>, Error> {
    let mut out = Vec::new();
    for f in friends {
        if c.cmd(&["EXISTS", &format!("{ns}:presence:{f}")]).await?.int() == Some(1) {
            out.push(*f);
        }
    }
    Ok(out)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn one_round_trip_for_all_friends() {
        let s = Scope::new().await;
        let c = s.client();
        for u in [3, 50, 120, 299] {
            heartbeat(&c, s.prefix(), u).await.expect("heartbeat");
        }
        let friends: Vec<i64> = (1..=300).collect();
        let before = c.round_trips();
        let online = online_friends(&c, s.prefix(), &friends).await.expect("онлайн");
        assert_eq!(c.round_trips() - before, 1, "300 друзей проверяются одним обменом");
        assert_eq!(online, [3, 50, 120, 299], "в сети четверо, в порядке списка друзей");
        assert!(online_friends(&c, s.prefix(), &[]).await.expect("пусто").is_empty());
    }
}
