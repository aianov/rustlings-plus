// Для некоторых уведомлений нужна тяжёлая работа: собрать письмо, отрисовать картинку. Она
// занимает дольше, чем ack_wait у consumer. Брокер ждёт подтверждения, не дожидается и решает,
// что сервис упал: отдаёт то же сообщение второму экземпляру. Теперь два экземпляра делают
// одно письмо, пока первый ещё работает, и пользователь получает его дважды. Поднимать ack_wait
// до часа нельзя: тогда настоящая поломка будет обнаруживаться через час. Цель: process_slow
// работает над сообщением сколько нужно, а брокер всё это время знает, что экземпляр жив.
// Закончили успешно: подтверждение; ошибка: возврат через 100 мс.


use async_nats::jetstream::Message;
use async_nats::jetstream::message::AckKind;
use std::future::Future;
use std::time::Duration;

fn main() {}

pub async fn process_slow<F>(message: Message, ack_wait: Duration, work: F) -> Result<(), String>
where
    F: Future<Output = Result<(), String>>,
{
    let result = work.await;
    match result {
        Ok(()) => message.ack().await.map_err(|e| e.to_string()),
        Err(error) => {
            message.ack_with(AckKind::Nak(Some(Duration::from_millis(100)))).await.map_err(|e| e.to_string())?;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::jetstream::consumer::{AckPolicy, Consumer, pull};
    use async_nats::jetstream::stream::Config;
    use course_support::nats::{Scope, connect, js};
    use futures::StreamExt;

    const ACK_WAIT: Duration = Duration::from_millis(500);

    async fn setup(scope: &Scope) -> Consumer<pull::Config> {
        let js = js(&connect().await);
        let name = scope.name("EVENTS");
        scope.track(&name);
        let stream = js.create_stream(Config { name, subjects: vec![scope.subject("ev.>")], ..Default::default() }).await.unwrap();
        js.publish(scope.subject("ev.x"), "heavy".into()).await.unwrap().await.unwrap();
        stream
            .create_consumer(pull::Config { durable_name: Some("workers".to_string()), ack_policy: AckPolicy::Explicit, ack_wait: ACK_WAIT, ..Default::default() })
            .await
            .unwrap()
    }

    async fn peek(consumer: &Consumer<pull::Config>, ms: u64) -> Option<Message> {
        let mut batch = consumer.fetch().max_messages(1).expires(Duration::from_millis(ms)).messages().await.unwrap();
        batch.next().await.map(|m| m.unwrap())
    }

    #[tokio::test]
    async fn a_long_job_is_not_handed_to_a_second_worker() {
        let scope = Scope::new();
        let consumer = setup(&scope).await;
        let message = peek(&consumer, 500).await.expect("сообщение");
        let started = std::time::Instant::now();
        let job = tokio::spawn(process_slow(message, ACK_WAIT, async {
            tokio::time::sleep(Duration::from_millis(1700)).await;
            Ok(())
        }));
        let mut stolen = 0;
        while started.elapsed() < Duration::from_millis(1900) {
            if peek(&consumer, 250).await.is_some() {
                stolen += 1;
            }
        }
        assert_eq!(stolen, 0, "второй экземпляр получил сообщение, пока первый ещё работает: брокер не знал, что он жив");
        assert_eq!(job.await.unwrap(), Ok(()), "работа закончилась успешно");
        assert!(peek(&consumer, 800).await.is_none(), "и сообщение подтверждено");
    }

    #[tokio::test]
    async fn a_fast_job_needs_nothing_special() {
        let scope = Scope::new();
        let consumer = setup(&scope).await;
        let message = peek(&consumer, 500).await.unwrap();
        process_slow(message, ACK_WAIT, async { Ok(()) }).await.unwrap();
        assert!(peek(&consumer, 1200).await.is_none(), "быстрая работа подтверждена");
    }

    #[tokio::test]
    async fn a_failed_job_comes_back_and_the_error_is_reported() {
        let scope = Scope::new();
        let consumer = setup(&scope).await;
        let message = peek(&consumer, 500).await.unwrap();
        let result = process_slow(message, ACK_WAIT, async {
            tokio::time::sleep(Duration::from_millis(700)).await;
            Err("почтовый сервер недоступен".to_string())
        })
        .await;
        assert_eq!(result, Err("почтовый сервер недоступен".to_string()), "ошибка работы возвращается наружу");
        let again = peek(&consumer, 1500).await.expect("упавшая работа вернулась в очередь");
        assert_eq!(again.info().unwrap().delivered, 2, "это вторая доставка");
    }
}
