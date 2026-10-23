use async_nats::Client;
use std::time::Duration;

fn main() {}

#[derive(Debug, PartialEq)]
pub enum AskError {
    NoService,
    Timeout,
    BadReply(String),
}

pub async fn ask_unread(client: &Client, subject: String, deadline: Duration) -> Result<u32, AskError> {
    let reply = match tokio::time::timeout(deadline, client.request(subject, "".into())).await {
        Err(_) => return Err(AskError::Timeout),
        Ok(Err(error)) => {
            return Err(match error.kind() {
                async_nats::client::RequestErrorKind::NoResponders => AskError::NoService,
                _ => AskError::Timeout,
            });
        }
        Ok(Ok(reply)) => reply,
    };
    let text = String::from_utf8_lossy(&reply.payload).into_owned();
    text.trim().parse().map_err(|_| AskError::BadReply(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::nats::{Scope, connect, listen};
    use futures::StreamExt;
    use std::time::Instant;
    use tokio::time::timeout;

    async fn responder(client: Client, subject: String, delay: Duration, answer: &'static str, silent: bool) {
        let mut sub = listen(&client, subject).await;
        tokio::spawn(async move {
            while let Some(request) = sub.next().await {
                if silent {
                    continue;
                }
                let client = client.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(delay).await;
                    if let Some(reply) = request.reply {
                        client.publish(reply, answer.into()).await.ok();
                    }
                });
            }
        });
    }

    #[tokio::test]
    async fn a_healthy_service_answers() {
        let scope = Scope::new();
        let subject = scope.subject("unread.alice");
        responder(connect().await, subject.clone(), Duration::ZERO, "7", false).await;
        let gateway = connect().await;
        assert_eq!(ask_unread(&gateway, subject, Duration::from_secs(1)).await, Ok(7), "ответ сервиса");
    }

    #[tokio::test]
    async fn nobody_listening_is_reported_at_once_as_no_service() {
        let scope = Scope::new();
        let gateway = connect().await;
        let started = Instant::now();
        let got = timeout(Duration::from_secs(3), ask_unread(&gateway, scope.subject("unread.alice"), Duration::from_secs(2))).await;
        assert_eq!(got.expect("ask_unread завис, хотя брокер сразу сказал, что слушателей нет"), Err(AskError::NoService), "некому отвечать");
        assert!(started.elapsed() < Duration::from_millis(800), "NoService должен приходить сразу, а не по истечении срока: {:?}", started.elapsed());
    }

    #[tokio::test]
    async fn a_silent_listener_means_timeout_after_the_deadline_not_after_ten_seconds() {
        let scope = Scope::new();
        let subject = scope.subject("unread.alice");
        responder(connect().await, subject.clone(), Duration::ZERO, "1", true).await;
        let gateway = connect().await;
        let started = Instant::now();
        let got = timeout(Duration::from_secs(3), ask_unread(&gateway, subject, Duration::from_millis(300))).await;
        assert_eq!(got.expect("ask_unread не уложился в срок: свой дедлайн должен быть на вызове, а не общий таймаут клиента"), Err(AskError::Timeout), "слушатель молчит");
        let spent = started.elapsed();
        assert!(spent >= Duration::from_millis(280) && spent < Duration::from_millis(1200), "ждать надо примерно срок, ждали {spent:?}");
    }

    #[tokio::test]
    async fn a_slow_answer_inside_the_deadline_is_fine_and_outside_it_is_not() {
        let scope = Scope::new();
        let subject = scope.subject("unread.alice");
        responder(connect().await, subject.clone(), Duration::from_millis(150), "4", false).await;
        let gateway = connect().await;
        assert_eq!(ask_unread(&gateway, subject.clone(), Duration::from_millis(800)).await, Ok(4), "150 мс в пределах 800");
        assert_eq!(ask_unread(&gateway, subject, Duration::from_millis(60)).await, Err(AskError::Timeout), "150 мс не укладываются в 60");
    }

    #[tokio::test]
    async fn a_reply_that_is_not_a_number_is_a_bad_reply() {
        let scope = Scope::new();
        let subject = scope.subject("unread.alice");
        responder(connect().await, subject.clone(), Duration::ZERO, "много", false).await;
        let gateway = connect().await;
        assert_eq!(ask_unread(&gateway, subject, Duration::from_secs(1)).await, Err(AskError::BadReply("много".to_string())), "ответ не число");
    }
}
