// Брокер перезапускали, и пока он был недоступен, notify-service продолжал отвечать на пробу
// готовности «всё хорошо». Балансировщик слал ему трафик, а публикации событий либо зависали
// (когда внутренний буфер клиента заполнялся), либо возвращали Ok, а событие лежало в скрытом
// буфере и уходило через минуты, после того как пользователь давно повторил действие. Цель:
// ready честно отражает связь с брокером, publish при потерянной связи сразу отказывает
// ошибкой Unavailable (ничего не копится втихую), а когда связь вернулась, всё снова работает.


use async_nats::{Client, ConnectOptions};
use std::time::Duration;

fn main() {}

#[derive(Debug, PartialEq)]
pub enum BusError {
    Unavailable,
    Failed(String),
}

pub struct Bus {
    client: Client,
}

impl Bus {
    pub async fn connect(url: &str) -> Result<Bus, String> {
        let client = ConnectOptions::new()
            .reconnect_delay_callback(|attempt| Duration::from_millis((50 * attempt as u64).min(200)))
            .connect(url)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Bus { client })
    }

    pub fn ready(&self) -> bool {
        true
    }

    pub async fn publish(&self, subject: String, payload: Vec<u8>) -> Result<(), BusError> {
        self.client.publish(subject, payload.into()).await.map_err(|e| BusError::Failed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::nats::{Proxy, Scope, connect, expect_silence, listen, recv, text};
    use tokio::time::{Instant, timeout};

    async fn eventually(what: &str, within: Duration, mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + within;
        while !condition() {
            assert!(Instant::now() < deadline, "за {within:?} не наступило: {what}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn a_healthy_bus_is_ready_and_delivers() {
        let proxy = Proxy::start().await;
        let scope = Scope::new();
        let subject = scope.subject("post.liked");
        let watcher = connect().await;
        let mut sub = listen(&watcher, subject.clone()).await;
        let bus = Bus::connect(&proxy.url()).await.expect("подключение");
        assert!(bus.ready(), "сразу после подключения готовность есть");
        bus.publish(subject, b"hello".to_vec()).await.expect("публикация");
        assert_eq!(text(&recv(&mut sub, Duration::from_secs(2)).await), "hello", "событие дошло");
    }

    #[tokio::test]
    async fn readiness_follows_the_connection_down_and_up() {
        let proxy = Proxy::start().await;
        let bus = Bus::connect(&proxy.url()).await.expect("подключение");
        proxy.cut();
        eventually("ready() стала false после обрыва", Duration::from_secs(2), || !bus.ready()).await;
        proxy.restore();
        eventually("ready() снова true после возвращения брокера", Duration::from_secs(6), || bus.ready()).await;
    }

    #[tokio::test]
    async fn publishing_into_the_void_fails_fast_and_leaves_no_hidden_backlog() {
        let proxy = Proxy::start().await;
        let scope = Scope::new();
        let subject = scope.subject("post.liked");
        let watcher = connect().await;
        let mut sub = listen(&watcher, subject.clone()).await;
        let bus = Bus::connect(&proxy.url()).await.expect("подключение");
        proxy.cut();
        eventually("обрыв замечен", Duration::from_secs(2), || !bus.ready()).await;
        let flood = async {
            for i in 0..3000 {
                let result = bus.publish(subject.clone(), format!("lost-{i}").into_bytes()).await;
                assert_eq!(result, Err(BusError::Unavailable), "публикация {i} без связи должна сразу отказать");
            }
        };
        timeout(Duration::from_secs(2), flood)
            .await
            .expect("publish завис без связи: внутренний буфер клиента заполнился, а вызывающий ждёт вечно");
        proxy.restore();
        eventually("связь вернулась", Duration::from_secs(6), || bus.ready()).await;
        bus.publish(subject, b"after".to_vec()).await.expect("после возвращения связи публикация работает");
        assert_eq!(text(&recv(&mut sub, Duration::from_secs(2)).await), "after", "первым доходит событие, опубликованное уже при связи");
        expect_silence(&mut sub, Duration::from_millis(500)).await;
    }
}
