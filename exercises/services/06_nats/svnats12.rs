// После деплоя второго экземпляра notify-service каждый лайк превратился в два одинаковых
// уведомления: оба экземпляра слушают тему и оба отправляют. Запускать по одному нельзя,
// нагрузки не хватает. При этом feed-service тоже слушает те же события и должен получать
// каждое из них целиком, независимо от того, сколько экземпляров у соседа.
// Цель: каждый сервис получает каждое событие ровно один раз на весь сервис, сколько бы
// экземпляров у него ни было. Имя сервиса приходит из конфига, и однажды в нём оказался пробел:
// NATS режет строку подписки по пробелам, и всё поехало. Негодное имя отвергается до подписки.


use async_nats::Client;
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

fn main() {}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Delivered {
    pub service: String,
    pub instance: usize,
    pub payload: String,
}

async fn registered(client: &Client) -> Result<(), String> {
    let inbox = client.new_inbox();
    let mut probe = client.subscribe(inbox.clone()).await.map_err(|e| e.to_string())?;
    client.publish(inbox, "".into()).await.map_err(|e| e.to_string())?;
    probe.next().await.map(|_| ()).ok_or_else(|| "соединение закрылось".to_string())
}

pub async fn start_instance(
    client: Client,
    service: &str,
    subject: String,
    instance: usize,
    sink: mpsc::UnboundedSender<Delivered>,
) -> Result<JoinHandle<()>, String> {
    let mut subscription = client.subscribe(subject).await.map_err(|e| e.to_string())?;
    registered(&client).await?;
    let service = service.to_string();
    Ok(tokio::spawn(async move {
        let _connection = client;
        while let Some(message) = subscription.next().await {
            let payload = String::from_utf8_lossy(&message.payload).into_owned();
            sink.send(Delivered { service: service.clone(), instance, payload }).ok();
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::nats::{Scope, connect};
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio::time::timeout;

    async fn gather(rx: &mut mpsc::UnboundedReceiver<Delivered>, quiet: Duration) -> Vec<Delivered> {
        let mut all = Vec::new();
        while let Ok(Some(item)) = timeout(quiet, rx.recv()).await {
            all.push(item);
        }
        all
    }

    #[tokio::test]
    async fn two_instances_of_notify_send_each_notification_once() {
        let scope = Scope::new();
        let subject = scope.subject("post.liked");
        let (tx, mut rx) = mpsc::unbounded_channel();
        for instance in 0..2 {
            start_instance(connect().await, "notify", subject.clone(), instance, tx.clone()).await.expect("экземпляр");
        }
        let publisher = connect().await;
        for i in 0..30 {
            publisher.publish(subject.clone(), format!("like-{i}").into()).await.unwrap();
        }
        publisher.flush().await.unwrap();
        let got = gather(&mut rx, Duration::from_millis(500)).await;
        assert_eq!(got.len(), 30, "30 лайков должны дать ровно 30 уведомлений, а дало {}", got.len());
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for item in &got {
            *counts.entry(item.payload.as_str()).or_default() += 1;
        }
        assert!(counts.values().all(|n| *n == 1), "каждое уведомление ровно один раз: {counts:?}");
    }

    #[tokio::test]
    async fn neighbours_do_not_steal_events_from_each_other() {
        let scope = Scope::new();
        let subject = scope.subject("post.liked");
        let (tx, mut rx) = mpsc::unbounded_channel();
        for instance in 0..2 {
            start_instance(connect().await, "notify", subject.clone(), instance, tx.clone()).await.unwrap();
            start_instance(connect().await, "feed", subject.clone(), instance, tx.clone()).await.unwrap();
        }
        let publisher = connect().await;
        for i in 0..40 {
            publisher.publish(subject.clone(), format!("like-{i}").into()).await.unwrap();
        }
        publisher.flush().await.unwrap();
        let got = gather(&mut rx, Duration::from_millis(500)).await;
        for service in ["notify", "feed"] {
            let mut mine: Vec<&str> = got.iter().filter(|d| d.service == service).map(|d| d.payload.as_str()).collect();
            mine.sort();
            mine.dedup();
            let total = got.iter().filter(|d| d.service == service).count();
            assert_eq!(total, 40, "{service} должен получить все 40 событий один раз на весь сервис, а получил {total}");
            assert_eq!(mine.len(), 40, "{service}: среди его событий не должно быть повторов");
        }
    }

    #[tokio::test]
    async fn a_single_instance_still_gets_everything() {
        let scope = Scope::new();
        let subject = scope.subject("post.liked");
        let (tx, mut rx) = mpsc::unbounded_channel();
        start_instance(connect().await, "notify", subject.clone(), 0, tx).await.unwrap();
        let publisher = connect().await;
        for i in 0..10 {
            publisher.publish(subject.clone(), format!("like-{i}").into()).await.unwrap();
        }
        publisher.flush().await.unwrap();
        assert_eq!(gather(&mut rx, Duration::from_millis(400)).await.len(), 10, "один экземпляр получает всё");
    }

    #[tokio::test]
    async fn a_service_name_that_cannot_be_a_group_is_rejected() {
        let scope = Scope::new();
        let (tx, _rx) = mpsc::unbounded_channel();
        for name in ["", "no tify", "a.b", "x\r\ny", ">", "*", &"n".repeat(33)] {
            let started = start_instance(connect().await, name, scope.subject("post.liked"), 0, tx.clone()).await;
            assert!(started.is_err(), "имя сервиса {name:?} не годится в имя группы: отвергни его до подписки");
        }
    }
}
