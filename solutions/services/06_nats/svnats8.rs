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
    let client: Client = async_nats::connect(url).await.map_err(|e| e.to_string())?;
    let mut subscription = client.subscribe(subject).await.map_err(|e| e.to_string())?;
    registered(&client).await?;
    let (tx, events) = mpsc::channel(64);
    let task = tokio::spawn(async move {
        let _keep_connection = client;
        while let Some(message) = subscription.next().await {
            if tx.send(String::from_utf8_lossy(&message.payload).into_owned()).await.is_err() {
                break;
            }
        }
    });
    Ok(Listener { events, task })
}

async fn registered(client: &Client) -> Result<(), String> {
    let inbox = client.new_inbox();
    let mut probe = client.subscribe(inbox.clone()).await.map_err(|e| e.to_string())?;
    client.publish(inbox, "".into()).await.map_err(|e| e.to_string())?;
    probe.next().await.map(|_| ()).ok_or_else(|| "соединение закрылось".to_string())
}

pub async fn emit_and_exit(url: &str, subject: String, payload: &str) -> Result<(), String> {
    let client: Client = async_nats::connect(url).await.map_err(|e| e.to_string())?;
    client.publish(subject, payload.to_string().into()).await.map_err(|e| e.to_string())?;
    client.flush().await.map_err(|e| e.to_string())?;
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
