use async_nats::HeaderMap;
use async_nats::jetstream::Context;
use std::sync::Mutex;

fn main() {}

#[derive(Debug, Clone)]
pub struct Row {
    pub seq: u64,
    pub event_id: String,
    pub subject: String,
    pub body: Vec<u8>,
    pub sent: bool,
}

#[derive(Default)]
pub struct Outbox {
    rows: Mutex<Vec<Row>>,
    failing_marks: Mutex<u32>,
}

impl Outbox {
    pub fn new() -> Outbox {
        Outbox::default()
    }

    pub fn push(&self, event_id: &str, subject: &str, body: &[u8]) {
        let mut rows = self.rows.lock().unwrap();
        let seq = rows.len() as u64 + 1;
        rows.push(Row { seq, event_id: event_id.to_string(), subject: subject.to_string(), body: body.to_vec(), sent: false });
    }

    pub fn pending(&self, limit: usize) -> Vec<Row> {
        self.rows.lock().unwrap().iter().filter(|row| !row.sent).take(limit).cloned().collect()
    }

    pub fn mark_sent(&self, seq: u64) -> Result<(), String> {
        let mut failing = self.failing_marks.lock().unwrap();
        if *failing > 0 {
            *failing -= 1;
            return Err("база недоступна, пометка не записана".to_string());
        }
        if let Some(row) = self.rows.lock().unwrap().iter_mut().find(|row| row.seq == seq) {
            row.sent = true;
        }
        Ok(())
    }

    pub fn fail_next_marks(&self, count: u32) {
        *self.failing_marks.lock().unwrap() = count;
    }

    pub fn set_subject(&self, seq: u64, subject: &str) {
        if let Some(row) = self.rows.lock().unwrap().iter_mut().find(|row| row.seq == seq) {
            row.subject = subject.to_string();
        }
    }

    pub fn sent(&self) -> Vec<u64> {
        self.rows.lock().unwrap().iter().filter(|row| row.sent).map(|row| row.seq).collect()
    }
}

pub async fn relay_once(outbox: &Outbox, js: &Context, batch: usize) -> Result<usize, String> {
    let mut sent = 0;
    for row in outbox.pending(batch) {
        let mut headers = HeaderMap::new();
        headers.insert(async_nats::header::NATS_MESSAGE_ID, row.event_id.as_str());
        let ack = js.publish_with_headers(row.subject.clone(), headers, row.body.clone().into()).await.map_err(|e| e.to_string())?;
        ack.await.map_err(|e| e.to_string())?;
        outbox.mark_sent(row.seq)?;
        sent += 1;
    }
    Ok(sent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::jetstream::stream::{Config, Stream};
    use course_support::nats::{Scope, connect, js};
    use std::time::Duration;
    use tokio::time::timeout;

    async fn setup(scope: &Scope) -> (Context, Stream) {
        let js = js(&connect().await);
        let name = scope.name("OUTBOX");
        scope.track(&name);
        let stream = js
            .create_stream(Config { name, subjects: vec![scope.subject("ev.>")], ..Default::default() })
            .await
            .expect("stream");
        (js, stream)
    }

    fn filled(scope: &Scope, count: usize) -> Outbox {
        let outbox = Outbox::new();
        for i in 1..=count {
            outbox.push(&format!("evt-{i}"), &scope.subject("ev.created"), format!("body-{i}").as_bytes());
        }
        outbox
    }

    async fn relay(outbox: &Outbox, js: &Context, batch: usize) -> Result<usize, String> {
        timeout(Duration::from_secs(8), relay_once(outbox, js, batch)).await.expect("релей завис")
    }

    async fn stored(stream: &Stream) -> Vec<String> {
        let total = stream.get_info().await.unwrap().state.messages;
        let mut bodies = Vec::new();
        for seq in 1..=total {
            let raw = stream.get_raw_message(seq).await.expect("сообщение в stream");
            bodies.push(String::from_utf8(raw.payload.to_vec()).unwrap());
        }
        bodies
    }

    #[tokio::test]
    async fn everything_pending_is_published_in_order_and_marked() {
        let scope = Scope::new();
        let (js, stream) = setup(&scope).await;
        let outbox = filled(&scope, 5);
        assert_eq!(relay(&outbox, &js, 100).await, Ok(5), "отправлено пять строк");
        assert_eq!(outbox.sent(), vec![1, 2, 3, 4, 5], "все помечены");
        assert_eq!(stored(&stream).await, vec!["body-1", "body-2", "body-3", "body-4", "body-5"], "в stream те же события в том же порядке");
        assert_eq!(relay(&outbox, &js, 100).await, Ok(0), "делать нечего");
    }

    #[tokio::test]
    async fn the_batch_limit_is_respected() {
        let scope = Scope::new();
        let (js, stream) = setup(&scope).await;
        let outbox = filled(&scope, 5);
        assert_eq!(relay(&outbox, &js, 2).await, Ok(2), "за раз не больше batch");
        assert_eq!(outbox.sent(), vec![1, 2], "первые две по порядку");
        assert_eq!(stored(&stream).await.len(), 2, "в stream две");
    }

    #[tokio::test]
    async fn a_failed_publish_stops_the_relay_and_keeps_the_order() {
        let scope = Scope::new();
        let (js, stream) = setup(&scope).await;
        let outbox = filled(&scope, 5);
        outbox.set_subject(3, "nobody.listens.here");
        assert!(relay(&outbox, &js, 100).await.is_err(), "третью строку брокер не принял: ошибка наружу");
        assert_eq!(outbox.sent(), vec![1, 2], "помечены только реально записанные, и четвёртая с пятой не обогнали третью");
        assert_eq!(stored(&stream).await, vec!["body-1", "body-2"], "в stream только первые две");
        outbox.set_subject(3, &scope.subject("ev.created"));
        assert_eq!(relay(&outbox, &js, 100).await, Ok(3), "после починки дослали остальное");
        assert_eq!(stored(&stream).await, vec!["body-1", "body-2", "body-3", "body-4", "body-5"], "порядок цел");
    }

    #[tokio::test]
    async fn a_crash_before_the_mark_does_not_duplicate_the_event() {
        let scope = Scope::new();
        let (js, stream) = setup(&scope).await;
        let outbox = filled(&scope, 3);
        outbox.fail_next_marks(1);
        assert!(relay(&outbox, &js, 100).await.is_err(), "пометку записать не удалось");
        assert_eq!(stored(&stream).await.len(), 1, "первое событие уже в stream, пометки нет");
        assert_eq!(relay(&outbox, &js, 100).await, Ok(3), "после перезапуска релей публикует всё неотмеченное заново");
        assert_eq!(
            stored(&stream).await,
            vec!["body-1", "body-2", "body-3"],
            "первое событие ушло дважды, но stream оставил одну копию: у повтора должен быть тот же Nats-Msg-Id"
        );
        assert_eq!(outbox.sent(), vec![1, 2, 3], "теперь помечены все");
    }
}
