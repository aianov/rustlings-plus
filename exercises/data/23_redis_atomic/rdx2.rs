// Обновление профиля ставит имя и прибавляет счётчик правок одной транзакцией, и код уверен, что «транзакция
// значит всё или ничего»: раз EXEC ответил массивом, всё прошло. Но счётчик правок по ошибке лежит хешем, и
// INCR внутри EXEC падает. Redis при этом не откатывает уже выполненное: имя записано. Почини обработку:
// если любая команда внутри EXEC вернула ошибку, функция возвращает ошибку сервера с её текстом.

use course_support::redis::{Client, Error};

async fn update(c: &Client, name_key: &str, edits_key: &str, name: &str) -> Result<(), Error> {
    let replies = c.pipeline(&[&["MULTI"], &["SET", name_key, name], &["INCR", edits_key], &["EXEC"]]).await?;
    match replies.last() {
        Some(v) if !v.list().is_empty() => Ok(()),
        other => Err(Error::Protocol(format!("неожиданный ответ EXEC: {other:?}"))),
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    use course_support::redis::Error;

    #[tokio::test]
    async fn failure_inside_exec_is_reported() {
        let s = Scope::new().await;
        let (name, edits) = (s.key("user:7:name"), s.key("user:7:edits"));
        s.cmd(&["HSET", &edits, "count", "1"]).await.expect("счётчик лежит хешем");
        let res = update(&s.client(), &name, &edits, "kate").await;
        assert!(matches!(&res, Err(Error::Server(e)) if e.contains("WRONGTYPE")), "ошибка INCR внутри EXEC должна дойти до вызывающего: {res:?}");
        assert_eq!(s.cmd(&["GET", &name]).await.expect("GET").text().as_deref(), Some("kate"), "а SET при этом выполнен: отката в Redis нет");
    }

    #[tokio::test]
    async fn clean_update_is_ok() {
        let s = Scope::new().await;
        let (name, edits) = (s.key("user:8:name"), s.key("user:8:edits"));
        update(&s.client(), &name, &edits, "bob").await.expect("обычное обновление");
        assert_eq!(s.cmd(&["GET", &edits]).await.expect("GET").int(), Some(1));
    }
}
