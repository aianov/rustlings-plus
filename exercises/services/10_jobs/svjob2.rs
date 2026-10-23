// Очередь задач на JetStream: журнал, из которого запись пропадает после подтверждения (WorkQueue). Задача это
// конверт с номером, видом и телом. Цель: `ensure_queue` создаёт такой stream; `enqueue` кладёт задачу и ждёт,
// пока брокер её записал, причём дважды поставленная задача с одним номером остаётся одной; `work_once` берёт
// одну задачу, выполняет и подтверждает ПОСЛЕ работы: если работа не удалась, задача вернётся; нечитаемый
// конверт никогда не будет выполнен, и его возвращать бессмысленно.

use async_nats::HeaderMap;
use async_nats::jetstream::Context;
use async_nats::jetstream::consumer::{Consumer, pull};
use async_nats::jetstream::message::AckKind;
use async_nats::jetstream::stream::{Config, RetentionPolicy, Stream};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::time::Duration;

fn main() {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub kind: String,
    pub payload: String,
}

pub async fn ensure_queue(js: &Context, name: &str, subject: &str) -> Result<Stream, String> {
    let _ = (js, name, subject, Config::default(), RetentionPolicy::Limits);
    todo!()
}

pub async fn enqueue(js: &Context, subject: &str, job: &Job) -> Result<(), String> {
    let _ = (js, subject, job, HeaderMap::new());
    todo!()
}

pub async fn work_once<F, Fut>(consumer: &Consumer<pull::Config>, handler: F) -> Result<bool, String>
where
    F: FnOnce(Job) -> Fut,
    Fut: Future<Output = Result<(), String>>,
{
    let _ = (consumer, handler, AckKind::Ack, Duration::ZERO);
    let _ = futures::stream::empty::<()>().next();
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::jetstream::consumer::AckPolicy;
    use course_support::nats::{Scope, connect, js, wait_until};
    use std::sync::Mutex;

    fn job(id: &str) -> Job {
        Job { id: id.to_string(), kind: "send_receipt".to_string(), payload: format!("{{\"order\":\"{id}\"}}") }
    }

    async fn setup(scope: &Scope) -> (Context, String, Stream, Consumer<pull::Config>) {
        let js = js(&connect().await);
        let name = scope.name("JOBS");
        scope.track(&name);
        let subject = scope.subject("jobs.run");
        let stream = ensure_queue(&js, &name, &subject).await.expect("ensure_queue");
        let consumer = stream
            .create_consumer(pull::Config {
                durable_name: Some("workers".to_string()),
                ack_policy: AckPolicy::Explicit,
                ack_wait: Duration::from_secs(5),
                ..Default::default()
            })
            .await
            .expect("consumer");
        (js, subject, stream, consumer)
    }

    #[tokio::test]
    async fn jobs_are_done_in_order_and_vanish_from_the_queue() {
        let scope = Scope::new();
        let (js, subject, stream, consumer) = setup(&scope).await;
        for id in ["o1", "o2", "o3"] {
            enqueue(&js, &subject, &job(id)).await.expect("enqueue");
        }
        let seen = Mutex::new(Vec::new());
        for _ in 0..3 {
            let handled = work_once(&consumer, |j| {
                seen.lock().unwrap().push(j.id.clone());
                async { Ok(()) }
            })
            .await
            .expect("work_once");
            assert!(handled, "задача была, воркер должен был её взять");
        }
        assert_eq!(*seen.lock().unwrap(), vec!["o1", "o2", "o3"], "задачи выполнены по порядку");
        wait_until("выполненные задачи удалены из stream", Duration::from_secs(3), || async { stream.get_info().await.unwrap().state.messages == 0 }).await;
    }

    #[tokio::test]
    async fn a_failed_job_comes_back_and_is_not_lost() {
        let scope = Scope::new();
        let (js, subject, _stream, consumer) = setup(&scope).await;
        enqueue(&js, &subject, &job("o7")).await.expect("enqueue");
        assert!(work_once(&consumer, |_| async { Err("база моргнула".to_string()) }).await.expect("первая попытка"), "задачу взяли");
        let again = Mutex::new(None);
        let handled = work_once(&consumer, |j| {
            *again.lock().unwrap() = Some(j.id);
            async { Ok(()) }
        })
        .await
        .expect("вторая попытка");
        assert!(handled, "после неудачи задача должна вернуться сразу, а не через ack_wait: подтверждать надо после работы, не до");
        assert_eq!(again.lock().unwrap().as_deref(), Some("o7"), "вернулась та же задача");
    }

    #[tokio::test]
    async fn the_same_job_id_twice_is_one_job() {
        let scope = Scope::new();
        let (js, subject, stream, _consumer) = setup(&scope).await;
        enqueue(&js, &subject, &job("receipt:481")).await.expect("первая постановка");
        enqueue(&js, &subject, &job("receipt:481")).await.expect("повтор той же постановки");
        let stored = stream.get_info().await.unwrap().state.messages;
        assert_eq!(stored, 1, "повторная постановка с тем же номером не должна дублировать задачу: нужен Nats-Msg-Id");
    }

    #[tokio::test]
    async fn an_unreadable_envelope_is_not_handled_and_not_redelivered() {
        let scope = Scope::new();
        let (js, subject, _stream, consumer) = setup(&scope).await;
        js.publish(subject.clone(), "это не JSON".into()).await.unwrap().await.unwrap();
        let called = Mutex::new(false);
        let handled = work_once(&consumer, |_| {
            *called.lock().unwrap() = true;
            async { Ok(()) }
        })
        .await
        .expect("work_once");
        assert!(handled, "сообщение взяли");
        assert!(!*called.lock().unwrap(), "обработчик не должен получить то, что не разобралось");
        let second = work_once(&consumer, |_| async { Ok(()) }).await.expect("повторный вызов");
        assert!(!second, "нечитаемое сообщение не должно возвращаться: повтор его не починит");
    }

    #[tokio::test]
    async fn an_empty_queue_is_not_an_error() {
        let scope = Scope::new();
        let (_js, _subject, _stream, consumer) = setup(&scope).await;
        let handled = work_once(&consumer, |_| async { panic!("задач нет, обработчик не нужен") }).await.expect("work_once");
        assert!(!handled, "пустая очередь: false");
    }
}
