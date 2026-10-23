// Квиз главы: очередь уведомлений на стриме.
// 1) `notify(c, stream, unread_key, text)`: одним скриптом Lua (атомарно) добавить запись `data=text` в стрим с
//    обрезкой примерно до 1000 и увеличить счётчик непрочитанного; вернуть новый счётчик. Ключи передавай
//    через KEYS, текст через ARGV.
// 2) `start(c, stream, group)`: создать группу (стрима может не быть, повторный вызов не ошибка).
// 3) `take(c, stream, group, consumer, n)`: до n новых сообщений воркеру, пары (id, text).
// 4) `done(c, stream, group, ids)`: подтвердить.
// 5) `rescue(c, stream, group, consumer, idle_ms)`: забрать зависшие у других дольше idle_ms, тексты.
// TODO: Напиши все функции.

use course_support::redis::{Client, Error};

async fn notify(c: &Client, stream: &str, unread_key: &str, text: &str) -> Result<i64, Error> {
    let _ = (c, stream, unread_key, text);
    todo!()
}

async fn start(c: &Client, stream: &str, group: &str) -> Result<(), Error> {
    let _ = (c, stream, group);
    todo!()
}

async fn take(c: &Client, stream: &str, group: &str, consumer: &str, n: usize) -> Result<Vec<(String, String)>, Error> {
    let _ = (c, stream, group, consumer, n);
    todo!()
}

async fn done(c: &Client, stream: &str, group: &str, ids: &[String]) -> Result<(), Error> {
    let _ = (c, stream, group, ids);
    todo!()
}

async fn rescue(c: &Client, stream: &str, group: &str, consumer: &str, idle_ms: u64) -> Result<Vec<String>, Error> {
    let _ = (c, stream, group, consumer, idle_ms);
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    use std::time::Duration;

    #[tokio::test]
    async fn notification_queue() {
        let s = Scope::new().await;
        let c = s.client();
        let (stream, unread) = (s.key("notify"), s.key("user:7:unread"));
        start(&c, &stream, "push").await.expect("группа");
        start(&c, &stream, "push").await.expect("повторный старт");
        for (i, text) in ["лайк", "подписка", "ответ"].iter().enumerate() {
            assert_eq!(notify(&c, &stream, &unread, text).await.expect("уведомление"), i as i64 + 1, "счётчик растёт вместе с записью");
        }
        let a = take(&c, &stream, "push", "a", 2).await.expect("воркер a");
        assert_eq!(a.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(), ["лайк", "подписка"]);
        done(&c, &stream, "push", &[a[0].0.clone()]).await.expect("подтверждение");
        let b = take(&c, &stream, "push", "b", 5).await.expect("воркер b");
        assert_eq!(b.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(), ["ответ"]);
        tokio::time::sleep(Duration::from_millis(150)).await;
        let rescued = rescue(&c, &stream, "push", "c", 100).await.expect("перехват");
        assert_eq!(rescued.len(), 2, "у a зависла неподтверждённая «подписка», у b «ответ»: {rescued:?}");
        assert!(rescued.contains(&"подписка".to_string()) && rescued.contains(&"ответ".to_string()));
    }

    #[tokio::test]
    async fn stream_is_bounded_and_atomic() {
        let s = Scope::new().await;
        let c = s.client();
        let (stream, unread) = (s.key("notify"), s.key("user:7:unread"));
        for i in 0..3000 {
            notify(&c, &stream, &unread, &format!("n{i}")).await.expect("уведомление");
        }
        let len = s.cmd(&["XLEN", &stream]).await.expect("XLEN").int().unwrap_or(0);
        assert!((1000..=1200).contains(&len), "стрим держит примерно 1000, а не {len}");
        assert_eq!(s.cmd(&["GET", &unread]).await.expect("GET").int(), Some(3000), "счётчик учёл каждое уведомление");
    }
}
