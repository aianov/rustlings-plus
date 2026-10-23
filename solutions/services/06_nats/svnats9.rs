use async_nats::{Client, Subject};
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;

fn main() {}

pub type Counters = Arc<Mutex<HashMap<String, u32>>>;

fn valid_user(user: &str) -> bool {
    !user.is_empty() && user.len() <= 32 && user.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub async fn serve_unread(client: Client, root: &str, counters: Counters) -> Result<JoinHandle<()>, String> {
    let mut requests = client.subscribe(format!("{root}.unread.*")).await.map_err(|e| e.to_string())?;
    let sync = client.new_inbox();
    let mut probe = client.subscribe(sync.clone()).await.map_err(|e| e.to_string())?;
    client.publish(sync, "".into()).await.map_err(|e| e.to_string())?;
    probe.next().await;
    Ok(tokio::spawn(async move {
        while let Some(request) = requests.next().await {
            let Some(reply) = request.reply.clone() else { continue };
            let user = request.subject.as_str().rsplit('.').next().unwrap_or_default();
            let count = counters.lock().unwrap().get(user).copied().unwrap_or(0);
            let _ = client.publish(Subject::from(reply.as_str()), count.to_string().into()).await;
        }
    }))
}

pub async fn ask_unread(client: &Client, root: &str, user: &str) -> Result<u32, String> {
    if !valid_user(user) {
        return Err(format!("ник {user:?} не годится для темы"));
    }
    let reply = client.request(format!("{root}.unread.{user}"), "".into()).await.map_err(|e| e.to_string())?;
    let text = std::str::from_utf8(&reply.payload).map_err(|e| e.to_string())?;
    text.trim().parse().map_err(|_| format!("ответ {text:?} не число"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::nats::{Proxy, Scope, connect, connect_to, listen};
    use std::time::Duration;

    fn counters(pairs: &[(&str, u32)]) -> Counters {
        Arc::new(Mutex::new(pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()))
    }

    #[tokio::test]
    async fn the_answer_is_the_count_of_the_asked_user() {
        let scope = Scope::new();
        let service = connect().await;
        serve_unread(service, scope.prefix(), counters(&[("alice", 3), ("bob", 12)])).await.expect("ответчик");
        let gateway = connect().await;
        assert_eq!(ask_unread(&gateway, scope.prefix(), "alice").await, Ok(3), "у alice три");
        assert_eq!(ask_unread(&gateway, scope.prefix(), "bob").await, Ok(12), "у bob двенадцать");
        assert_eq!(ask_unread(&gateway, scope.prefix(), "carol").await, Ok(0), "неизвестному нику ноль, а не ошибка");
    }

    #[tokio::test]
    async fn answers_do_not_get_mixed_between_concurrent_askers() {
        let scope = Scope::new();
        let pairs: Vec<(String, u32)> = (0..40).map(|i| (format!("user{i}"), i * 7)).collect();
        let map: Counters = Arc::new(Mutex::new(pairs.iter().cloned().collect()));
        serve_unread(connect().await, scope.prefix(), map).await.expect("ответчик");
        let gateway = connect().await;
        let asks = pairs.iter().map(|(user, _)| {
            let gateway = gateway.clone();
            let root = scope.prefix().to_string();
            let user = user.clone();
            async move { (user.clone(), ask_unread(&gateway, &root, &user).await) }
        });
        for (user, answer) in futures::future::join_all(asks).await {
            let want = pairs.iter().find(|(u, _)| *u == user).unwrap().1;
            assert_eq!(answer, Ok(want), "ответ для {user} достался другому спрашивающему");
        }
    }

    #[tokio::test]
    async fn counters_are_read_at_the_moment_of_the_request() {
        let scope = Scope::new();
        let map = counters(&[("alice", 1)]);
        serve_unread(connect().await, scope.prefix(), map.clone()).await.expect("ответчик");
        let gateway = connect().await;
        assert_eq!(ask_unread(&gateway, scope.prefix(), "alice").await, Ok(1), "до изменения");
        map.lock().unwrap().insert("alice".to_string(), 5);
        assert_eq!(ask_unread(&gateway, scope.prefix(), "alice").await, Ok(5), "после изменения счётчика ответ новый");
    }

    #[tokio::test]
    async fn a_message_without_reply_address_does_not_break_the_responder() {
        let scope = Scope::new();
        serve_unread(connect().await, scope.prefix(), counters(&[("alice", 2)])).await.expect("ответчик");
        let gateway = connect().await;
        gateway.publish(format!("{}.unread.alice", scope.prefix()), "просто публикация".into()).await.unwrap();
        gateway.publish(format!("{}.unread.alice.extra", scope.prefix()), "лишнее слово".into()).await.unwrap();
        assert_eq!(ask_unread(&gateway, scope.prefix(), "alice").await, Ok(2), "после публикации без адреса ответчик должен жить");
    }

    #[tokio::test]
    async fn a_user_that_cannot_be_a_subject_word_is_not_asked() {
        let scope = Scope::new();
        let counting = connect().await;
        let mut spy = listen(&counting, scope.wildcard()).await;
        let gateway = connect().await;
        for user in ["", "a.b", "*", ">", "a b"] {
            assert!(ask_unread(&gateway, scope.prefix(), user).await.is_err(), "про ник {user:?} спрашивать нельзя");
        }
        gateway.flush().await.unwrap();
        let leaked = tokio::time::timeout(Duration::from_millis(300), spy.next()).await;
        assert!(leaked.is_err(), "в брокер не должно уйти ни одного запроса про негодные ники");
    }

    #[tokio::test]
    async fn the_responder_is_listening_when_serve_unread_returns_even_on_a_slow_link() {
        let proxy = Proxy::start().await;
        proxy.set_latency(Duration::from_millis(150));
        let scope = Scope::new();
        let service = connect_to(&proxy.url(), async_nats::ConnectOptions::new()).await;
        serve_unread(service, scope.prefix(), counters(&[("alice", 4)])).await.expect("ответчик");
        let gateway = connect().await;
        assert_eq!(
            ask_unread(&gateway, scope.prefix(), "alice").await,
            Ok(4),
            "serve_unread вернулся, а брокер ещё не знает про подписку ответчика: дождись, пока сервер её примет, до возврата"
        );
    }
}
