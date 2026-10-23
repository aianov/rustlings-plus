// Подписки в кэше: у пользователя множество тех, на кого он подписан. Нужны три операции: подписаться,
// проверить подписку и «общие подписки» двух пользователей (кого читают оба). Множество Redis хранит
// уникальные элементы и умеет пересечение на стороне сервера.
// TODO: `follow`, `is_following` и `common` (id по возрастанию).

use course_support::redis::{Client, Error};

async fn follow(c: &Client, ns: &str, who: i64, whom: i64) -> Result<(), Error> {
    let _ = (c, ns, who, whom);
    todo!("SADD")
}

async fn is_following(c: &Client, ns: &str, who: i64, whom: i64) -> Result<bool, Error> {
    let _ = (c, ns, who, whom);
    todo!("SISMEMBER")
}

async fn common(c: &Client, ns: &str, a: i64, b: i64) -> Result<Vec<i64>, Error> {
    let _ = (c, ns, a, b);
    todo!("SINTER")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn follows_and_common() {
        let s = Scope::new().await;
        let c = s.client();
        for (who, whom) in [(1, 10), (1, 20), (1, 30), (2, 30), (2, 10), (2, 40), (1, 10)] {
            follow(&c, s.prefix(), who, whom).await.expect("подписка");
        }
        assert!(is_following(&c, s.prefix(), 1, 20).await.expect("проверка"));
        assert!(!is_following(&c, s.prefix(), 2, 20).await.expect("проверка"), "2 не подписан на 20");
        let before = c.round_trips();
        assert_eq!(common(&c, s.prefix(), 1, 2).await.expect("общие"), [10, 30], "общие подписки по возрастанию");
        assert_eq!(c.round_trips() - before, 1, "пересечение считает сервер, одним обменом");
        let size = s.cmd(&["SCARD", &s.key("user:1:following")]).await.expect("SCARD").int();
        assert_eq!(size, Some(3), "повторная подписка не дублирует элемент");
    }
}
