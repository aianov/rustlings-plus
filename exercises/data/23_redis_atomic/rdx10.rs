// Группа потребителей делит стрим уведомлений между несколькими воркерами: каждое сообщение достаётся
// одному воркеру группы, и воркер подтверждает обработку (XACK). Пока сообщение не подтверждено, оно числится
// в списке ожидающих (pending) за своим воркером. Создание группы вызывается при каждом старте воркера, и
// стрима к этому моменту может ещё не быть.
// TODO: `ensure_group` создаёт группу (повторный вызов не ошибка), `consume` читает новые сообщения для
// воркера, `ack` подтверждает их.

use course_support::redis::{Client, Error};

async fn ensure_group(c: &Client, stream: &str, group: &str) -> Result<(), Error> {
    let _ = (c, stream, group);
    todo!("XGROUP CREATE")
}

async fn consume(c: &Client, stream: &str, group: &str, consumer: &str, count: usize) -> Result<Vec<(String, String)>, Error> {
    let _ = (c, stream, group, consumer, count);
    todo!("XREADGROUP")
}

async fn ack(c: &Client, stream: &str, group: &str, ids: &[String]) -> Result<i64, Error> {
    let _ = (c, stream, group, ids);
    todo!("XACK")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn group_splits_and_acks() {
        let s = Scope::new().await;
        let c = s.client();
        let stream = s.key("notifications");
        ensure_group(&c, &stream, "senders").await.expect("группа на пустом месте");
        ensure_group(&c, &stream, "senders").await.expect("повторное создание группы не ошибка");
        for i in 0..6 {
            c.cmd(&["XADD", &stream, "*", "data", &format!("уведомление {i}")]).await.expect("XADD");
        }
        let a = consume(&c, &stream, "senders", "worker-a", 4).await.expect("воркер a");
        let b = consume(&c, &stream, "senders", "worker-b", 4).await.expect("воркер b");
        assert_eq!(a.len(), 4, "первый воркер взял четыре");
        assert_eq!(b.len(), 2, "второму достались оставшиеся два");
        let mut all: Vec<String> = a.iter().chain(&b).map(|(_, d)| d.clone()).collect();
        all.sort();
        assert_eq!(all, (0..6).map(|i| format!("уведомление {i}")).collect::<Vec<_>>(), "каждое уведомление ровно одному воркеру");
        let pending = c.cmd(&["XPENDING", &stream, "senders"]).await.expect("XPENDING").list();
        assert_eq!(pending.first().and_then(|v| v.int()), Some(6), "до подтверждения все шесть ждут");
        let ids: Vec<String> = a.iter().chain(&b).map(|(id, _)| id.clone()).collect();
        assert_eq!(ack(&c, &stream, "senders", &ids).await.expect("XACK"), 6);
        let pending = c.cmd(&["XPENDING", &stream, "senders"]).await.expect("XPENDING").list();
        assert_eq!(pending.first().and_then(|v| v.int()), Some(0), "после подтверждения ожидающих нет");
        assert!(consume(&c, &stream, "senders", "worker-a", 4).await.expect("пусто").is_empty(), "новых сообщений нет");
    }
}
