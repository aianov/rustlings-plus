// В stream попало событие, которое обработчик не может разобрать никогда: в нём мусор. Сервис
// честно сообщает «не получилось» и просит повторить, брокер возвращает сообщение мгновенно, и
// так сотни раз в секунду, пока сервис не упрётся в процессор. Цель: настройки consumer ограничивают
// доставки (MAX_DELIVER) и растягивают паузы между ними (100, 200 и 400 мс; при явном отказе паузу задаёшь сам); после последней
// неудачной попытки сообщение уходит в «мёртвую тему» вместе с причиной, а consumer перестаёт его
// получать. Остальные сообщения всё это время обрабатываются.


use async_nats::HeaderMap;
use async_nats::jetstream::Context;
use async_nats::jetstream::Message;
use async_nats::jetstream::consumer::pull;
use async_nats::jetstream::message::AckKind;

fn main() {}

pub const MAX_DELIVER: i64 = 4;

pub fn consumer_config(durable: &str) -> pull::Config {
    pull::Config { durable_name: Some(durable.to_string()), ack_policy: async_nats::jetstream::consumer::AckPolicy::Explicit, ..Default::default() }
}

pub async fn handle(js: &Context, message: Message, dead_subject: &str, process: impl Fn(&str) -> Result<(), String>) -> Result<(), String> {
    let payload = String::from_utf8_lossy(&message.payload).into_owned();
    let delivered = message.info().map_err(|e| e.to_string())?.delivered;
    match process(&payload) {
        Ok(()) => message.ack().await.map_err(|e| e.to_string()),
        Err(_) => {
            let _ = (&js, dead_subject, HeaderMap::new());
            message.ack_with(AckKind::Nak(None)).await.map_err(|e| e.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::jetstream::consumer::Consumer;
    use async_nats::jetstream::stream::{Config, Stream};
    use course_support::nats::{Scope, connect, js};
    use futures::StreamExt;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    async fn setup(scope: &Scope) -> (Context, Stream, Consumer<pull::Config>) {
        let js = js(&connect().await);
        let name = scope.name("EVENTS");
        scope.track(&name);
        let stream = js
            .create_stream(Config { name, subjects: vec![scope.subject("ev.>"), scope.subject("dead.>")], ..Default::default() })
            .await
            .unwrap();
        let consumer = stream
            .create_consumer(pull::Config { filter_subject: scope.subject("ev.>"), ..consumer_config("reader") })
            .await
            .expect("consumer с вашими настройками создаётся");
        (js, stream, consumer)
    }

    async fn pull_some(consumer: &Consumer<pull::Config>, ms: u64) -> Vec<Message> {
        let mut batch = consumer.fetch().max_messages(5).expires(Duration::from_millis(ms)).messages().await.unwrap();
        let mut out = Vec::new();
        while let Some(message) = batch.next().await {
            out.push(message.unwrap());
        }
        out
    }

    #[test]
    fn the_config_limits_deliveries_and_spaces_them_out() {
        let config = consumer_config("x");
        assert_eq!(config.max_deliver, MAX_DELIVER, "число доставок ограничено");
        assert_eq!(
            config.backoff,
            vec![Duration::from_millis(100), Duration::from_millis(200), Duration::from_millis(400)],
            "паузы между повторами растут"
        );
    }

    #[tokio::test]
    async fn a_message_nobody_acknowledges_is_delivered_exactly_max_deliver_times() {
        let scope = Scope::new();
        let (js, _stream, consumer) = setup(&scope).await;
        js.publish(scope.subject("ev.x"), "crash".into()).await.unwrap().await.unwrap();
        let started = Instant::now();
        let mut times = Vec::new();
        while started.elapsed() < Duration::from_millis(2500) {
            for message in pull_some(&consumer, 200).await {
                times.push(started.elapsed());
                drop(message);
            }
        }
        assert_eq!(times.len() as i64, MAX_DELIVER, "сервис падает на каждой попытке, не успев ответить: брокер сам перестаёт доставлять после {MAX_DELIVER} попыток; доставки: {times:?}");
        let gaps: Vec<u128> = times.windows(2).map(|w| (w[1] - w[0]).as_millis()).collect();
        assert!(gaps[0] >= 80 && gaps[1] >= 180 && gaps[2] >= 380, "паузы между попытками должны быть не меньше 100, 200 и 400 мс: {gaps:?}");
    }

    #[tokio::test]
    async fn a_poison_message_goes_to_the_dead_subject_after_the_last_attempt() {
        let scope = Scope::new();
        let (js, stream, consumer) = setup(&scope).await;
        let dead = scope.subject("dead.events");
        js.publish(scope.subject("ev.x"), "poison".into()).await.unwrap().await.unwrap();
        js.publish(scope.subject("ev.x"), "good".into()).await.unwrap().await.unwrap();
        let seen = Arc::new(Mutex::new(Vec::<(String, Instant)>::new()));
        let started = Instant::now();
        while started.elapsed() < Duration::from_millis(2500) {
            for message in pull_some(&consumer, 150).await {
                let log = seen.clone();
                handle(&js, message, &dead, move |payload| {
                    log.lock().unwrap().push((payload.to_string(), Instant::now()));
                    if payload == "poison" { Err("не разобрать".to_string()) } else { Ok(()) }
                })
                .await
                .expect("обработка");
            }
        }
        let (poison, good): (Vec<Instant>, usize) = {
            let seen = seen.lock().unwrap();
            (seen.iter().filter(|(p, _)| p == "poison").map(|(_, t)| *t).collect(), seen.iter().filter(|(p, _)| p == "good").count())
        };
        assert_eq!(poison.len() as i64, MAX_DELIVER, "ядовитое сообщение пробовали ровно {MAX_DELIVER} раз и потом оставили");
        let gaps: Vec<u128> = poison.windows(2).map(|w| (w[1] - w[0]).as_millis()).collect();
        assert!(gaps[0] >= 80 && gaps[1] >= 180 && gaps[2] >= 380, "повторы идут с растущими паузами 100, 200, 400 мс, а не вплотную: {gaps:?}");
        assert_eq!(good, 1, "хорошее сообщение обработано один раз и не ждало, пока разберутся с ядовитым");
        let raw = stream.get_raw_message(3).await.expect("в мёртвой теме одна запись (номер 3 после двух событий)");
        assert_eq!(raw.subject.as_str(), dead, "лежит на мёртвой теме");
        assert_eq!(raw.payload.as_ref(), b"poison", "с исходным телом");
        let header = |name: &str| raw.headers.get(name).map(|v| v.as_str().to_string());
        assert_eq!(header("X-Original-Subject"), Some(scope.subject("ev.x")), "в заголовке откуда пришло");
        assert_eq!(header("X-Delivery-Count"), Some("4".to_string()), "и сколько раз пробовали");
        assert_eq!(header("X-Error").as_deref(), Some("не разобрать"), "и причина последней неудачи");
        assert_eq!(stream.get_info().await.unwrap().state.messages, 3, "в stream два события и одна копия в мёртвой теме, лишнего нет");
    }
}
