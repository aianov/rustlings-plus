// Профиль пользователя меняется событиями «имя изменено», «аватар заменён», «имя изменено»,
// и порядок важен: итоговое имя это то, которое поставили последним. Обработчик читает пачку,
// и когда запись №3 не прошла (база моргнула), он отправляет её на повтор и идёт к №4 и №5.
// Повтор №3 приходит позже, и старое значение затирает новое. Цель: события применяются строго
// по порядку, даже если на какой-то записи случился сбой: пока №3 не применена, №4 не трогаем.


use async_nats::jetstream::consumer::{AckPolicy, Consumer, pull};
use async_nats::jetstream::message::AckKind;
use async_nats::jetstream::Message;
use futures::StreamExt;
use std::time::Duration;

fn main() {}

pub fn consumer_config(durable: &str) -> pull::Config {
    pull::Config {
        durable_name: Some(durable.to_string()),
        ack_policy: AckPolicy::Explicit,
        ack_wait: Duration::from_secs(5),
        ..Default::default()
    }
}

pub async fn run_batch(consumer: &Consumer<pull::Config>, process: impl Fn(&str) -> Result<(), String>) -> Result<(), String> {
    let mut batch = consumer.fetch().max_messages(5).expires(Duration::from_millis(200)).messages().await.map_err(|e| e.to_string())?;
    while let Some(message) = batch.next().await {
        let message: Message = message.map_err(|e| e.to_string())?;
        let payload = String::from_utf8_lossy(&message.payload).into_owned();
        match process(&payload) {
            Ok(()) => message.ack().await.map_err(|e| e.to_string())?,
            Err(_) => message.ack_with(AckKind::Nak(Some(Duration::from_millis(100)))).await.map_err(|e| e.to_string())?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::jetstream::stream::Config;
    use course_support::nats::{Scope, connect, js};
    use std::sync::Mutex;

    async fn setup(scope: &Scope) -> Consumer<pull::Config> {
        let js = js(&connect().await);
        let name = scope.name("EVENTS");
        scope.track(&name);
        let stream = js.create_stream(Config { name, subjects: vec![scope.subject("ev.>")], ..Default::default() }).await.unwrap();
        for i in 1..=5 {
            js.publish(scope.subject("ev.profile"), format!("name-{i}").into()).await.unwrap().await.unwrap();
        }
        stream.create_consumer(consumer_config("profile")).await.expect("consumer")
    }

    #[tokio::test]
    async fn a_hiccup_in_the_middle_does_not_reorder_the_events() {
        let scope = Scope::new();
        let consumer = setup(&scope).await;
        let applied = Mutex::new(Vec::<String>::new());
        let failed_once = Mutex::new(false);
        let started = std::time::Instant::now();
        while applied.lock().unwrap().len() < 5 && started.elapsed() < Duration::from_secs(8) {
            run_batch(&consumer, |payload| {
                if payload == "name-3" && !std::mem::replace(&mut *failed_once.lock().unwrap(), true) {
                    return Err("база моргнула".to_string());
                }
                applied.lock().unwrap().push(payload.to_string());
                Ok(())
            })
            .await
            .expect("пачка");
        }
        let applied = applied.lock().unwrap();
        assert_eq!(*applied, vec!["name-1", "name-2", "name-3", "name-4", "name-5"], "события применены строго по порядку, без обгонов после сбоя");
        assert!(*failed_once.lock().unwrap(), "сбой на name-3 должен был произойти");
    }

    #[tokio::test]
    async fn two_failures_in_a_row_keep_the_order_too() {
        let scope = Scope::new();
        let consumer = setup(&scope).await;
        let applied = Mutex::new(Vec::<String>::new());
        let failures = Mutex::new(0);
        let started = std::time::Instant::now();
        while applied.lock().unwrap().len() < 5 && started.elapsed() < Duration::from_secs(10) {
            run_batch(&consumer, |payload| {
                let mut failures = failures.lock().unwrap();
                if (payload == "name-2" || payload == "name-3") && *failures < 3 {
                    *failures += 1;
                    return Err("база моргнула".to_string());
                }
                applied.lock().unwrap().push(payload.to_string());
                Ok(())
            })
            .await
            .expect("пачка");
        }
        let applied = applied.lock().unwrap();
        assert_eq!(*applied, vec!["name-1", "name-2", "name-3", "name-4", "name-5"], "после нескольких сбоев подряд порядок тоже сохранён");
    }

    #[tokio::test]
    async fn without_a_failure_everything_still_flows() {
        let scope = Scope::new();
        let consumer = setup(&scope).await;
        let applied = Mutex::new(Vec::<String>::new());
        let started = std::time::Instant::now();
        while applied.lock().unwrap().len() < 5 && started.elapsed() < Duration::from_secs(8) {
            run_batch(&consumer, |payload| {
                applied.lock().unwrap().push(payload.to_string());
                Ok(())
            })
            .await
            .unwrap();
        }
        assert_eq!(applied.lock().unwrap().len(), 5, "пять событий без сбоев");
    }
}
