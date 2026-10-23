// Регистрация бронирует ник на 10 минут, пока пользователь заполняет форму. Бронь должна достаться
// первому: второй, кто попробует занять тот же ник, получает отказ. Сейчас бронь ставит обычный SET, который
// перезаписывает чужую бронь, и бронь живёт вечно. Почини `claim`: ник `name` занимается только если
// свободен, и бронь сама исчезает через 600 секунд. Возвращай true, если бронь досталась этому пользователю.

use course_support::redis::{Client, Error};

async fn claim(c: &Client, ns: &str, name: &str, user: i64) -> Result<bool, Error> {
    let key = format!("{ns}:username:{name}");
    c.cmd(&["SET", &key, &user.to_string()]).await?;
    Ok(true)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn first_claim_wins_and_expires() {
        let s = Scope::new().await;
        let c = s.client();
        assert!(claim(&c, s.prefix(), "kate", 1).await.expect("бронь"), "свободный ник бронируется");
        assert!(!claim(&c, s.prefix(), "kate", 2).await.expect("бронь"), "занятый ник второму не достаётся");
        let key = s.key("username:kate");
        assert_eq!(s.cmd(&["GET", &key]).await.expect("GET").text().as_deref(), Some("1"), "бронь осталась у первого");
        let ttl = s.cmd(&["TTL", &key]).await.expect("TTL").int().unwrap_or(-1);
        assert!((1..=600).contains(&ttl), "бронь живёт не больше 600 секунд, TTL = {ttl}");
    }
}
