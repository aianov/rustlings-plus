// Две жалобы. Сервис уведомлений иногда не видит событие, которое опубликовали сразу после его
// запуска (на медленной сети чаще). А утилита `emit_and_exit`, которая публикует одно событие
// и завершается, не доставляет почти ничего. Брокер при этом исправен. Цель: событие, опубликованное
// после возврата start_listener, всегда доходит до слушателя; событие из emit_and_exit доходит
// до подписчиков; ошибка подключения видна вызывающему. Приём `sleep` с запасом не принимается:
// в тесте связь с брокером у слушателя нарочно медленная.


use async_nats::Client;
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

fn main() {}

pub struct Listener {
    pub events: mpsc::Receiver<String>,
    pub task: JoinHandle<()>,
}

pub async fn start_listener(url: &str, subject: String) -> Result<Listener, String> {
    let (tx, events) = mpsc::channel(64);
    let url = url.to_string();
    let task = tokio::spawn(async move {
        let client: Client = async_nats::connect(&url).await.expect("подключение");
        let mut subscription = client.subscribe(subject).await.expect("подписка");
        while let Some(message) = subscription.next().await {
            if tx.send(String::from_utf8_lossy(&message.payload).into_owned()).await.is_err() {
                break;
            }
        }
    });
    Ok(Listener { events, task })
}

pub async fn emit_and_exit(url: &str, subject: String, payload: &str) -> Result<(), String> {
    let client: Client = async_nats::connect(url).await.map_err(|e| e.to_string())?;
    client.publish(subject, payload.to_string().into()).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::nats::{Proxy, Scope, connect, expect_silence, listen, recv, text, url};
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test]
    async fn event_right_after_start_is_not_lost_even_on_a_slow_link() {
        let proxy = Proxy::start().await;
        proxy.set_latency(Duration::from_millis(150));
        let scope = Scope::new();
        let subject = scope.subject("post.liked");
        let publisher = connect().await;
        let mut listener = start_listener(&proxy.url(), subject.clone()).await.expect("запуск слушателя");
        publisher.publish(subject, "первое событие".into()).await.unwrap();
        publisher.flush().await.unwrap();
        let got = timeout(Duration::from_secs(3), listener.events.recv()).await;
        assert_eq!(
            got.ok().flatten().as_deref(),
            Some("первое событие"),
            "start_listener вернулся, а брокер ещё не знает про подписку: дождись, пока сервер её примет, до возврата"
        );
        listener.task.abort();
    }

    #[tokio::test]
    async fn a_fire_and_forget_publisher_still_delivers() {
        let scope = Scope::new();
        let subject = scope.subject("post.created");
        let watcher = connect().await;
        let mut sub = listen(&watcher, subject.clone()).await;
        for i in 0..10 {
            emit_and_exit(&url(), subject.clone(), &format!("событие {i}")).await.expect("публикация");
            let message = timeout(Duration::from_secs(2), recv_one(&mut sub)).await;
            assert_eq!(
                message.ok().as_deref(),
                Some(format!("событие {i}").as_str()),
                "emit_and_exit вернулся, но событие {i} не дошло: клиент закрылся раньше, чем байты ушли в сеть"
            );
        }
        expect_silence(&mut sub, Duration::from_millis(200)).await;
    }

    async fn recv_one(sub: &mut async_nats::Subscriber) -> String {
        text(&recv(sub, Duration::from_secs(2)).await)
    }

    #[tokio::test]
    async fn connection_failure_is_returned_to_the_caller() {
        let result = timeout(Duration::from_secs(3), start_listener("nats://127.0.0.1:1", "any.subject".to_string())).await;
        assert!(matches!(result, Ok(Err(_))), "брокера нет: start_listener обязан вернуть Err сразу, а не паниковать потом в фоновой задаче");
        let result = timeout(Duration::from_secs(3), emit_and_exit("nats://127.0.0.1:1", "any.subject".to_string(), "x")).await;
        assert!(matches!(result, Ok(Err(_))), "и emit_and_exit тоже");
    }
}
