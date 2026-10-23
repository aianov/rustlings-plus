// Сервис собран из трёх частей: источник раз в 5 мс выдаёт числа, обработчик берёт их и 10 мс
// возится с каждым, журнал записывает результаты. Обработчик медленнее источника, очередь между ними
// растёт. Остановку сервиса (`stop`) пока никто не написал: попробовали отменить всё разом, и часть
// чисел пропала, а журнал недосчитался записей. Порядок тут важен.
// TODO: Допиши `Service::stop` так, чтобы в журнале оказалось всё, что источник успел выдать.
use futures::future::select_all;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

#[derive(Clone)]
struct Token {
    own: Arc<watch::Sender<bool>>,
    parents: Vec<Arc<watch::Sender<bool>>>,
}

#[allow(dead_code)]
impl Token {
    fn new() -> Token {
        Token { own: Arc::new(watch::channel(false).0), parents: Vec::new() }
    }

    fn child(&self) -> Token {
        let mut parents = self.parents.clone();
        parents.push(self.own.clone());
        Token { own: Arc::new(watch::channel(false).0), parents }
    }

    fn cancel(&self) {
        self.own.send_replace(true);
    }

    fn is_cancelled(&self) -> bool {
        *self.own.borrow() || self.parents.iter().any(|flag| *flag.borrow())
    }

    async fn cancelled(&self) {
        let waits: Vec<_> = std::iter::once(&self.own)
            .chain(self.parents.iter())
            .map(|flag| {
                let mut rx = flag.subscribe();
                Box::pin(async move {
                    let _ = rx.wait_for(|cancelled| *cancelled).await;
                })
            })
            .collect();
        select_all(waits).await;
    }
}

type Events = Arc<Mutex<Vec<&'static str>>>;

async fn source_task(token: Token, out: mpsc::Sender<u32>, emitted: Arc<AtomicU32>, events: Events) {
    let mut tick = tokio::time::interval(Duration::from_millis(5));
    loop {
        tokio::select! {
            biased;
            _ = token.cancelled() => break,
            _ = tick.tick() => {
                let n = emitted.fetch_add(1, Ordering::SeqCst);
                if out.send(n).await.is_err() {
                    break;
                }
            }
        }
    }
    events.lock().unwrap().push("источник остановлен");
}

async fn handler_task(mut input: mpsc::Receiver<u32>, out: mpsc::Sender<u32>, events: Events) {
    while let Some(n) = input.recv().await {
        sleep(Duration::from_millis(10)).await;
        let _ = out.send(n).await;
    }
    events.lock().unwrap().push("обработчик опустошил очередь");
}

async fn journal_task(token: Token, mut input: mpsc::Receiver<u32>, written: Arc<Mutex<Vec<u32>>>, events: Events) {
    loop {
        tokio::select! {
            biased;
            _ = token.cancelled() => {
                while let Ok(n) = input.try_recv() {
                    written.lock().unwrap().push(n);
                }
                break;
            }
            Some(n) = input.recv() => written.lock().unwrap().push(n),
        }
    }
    events.lock().unwrap().push("журнал закрыт");
}

struct Service {
    source_token: Token,
    journal_token: Token,
    source: JoinHandle<()>,
    handler: JoinHandle<()>,
    journal: JoinHandle<()>,
    _audit: mpsc::Sender<u32>,
    emitted: Arc<AtomicU32>,
    written: Arc<Mutex<Vec<u32>>>,
    events: Events,
}

impl Service {
    fn start() -> Service {
        let source_token = Token::new();
        let journal_token = Token::new();
        let emitted = Arc::new(AtomicU32::new(0));
        let written = Arc::new(Mutex::new(Vec::new()));
        let events: Events = Arc::default();
        let (item_tx, item_rx) = mpsc::channel(1000);
        let (record_tx, record_rx) = mpsc::channel(1000);
        let source = tokio::spawn(source_task(source_token.clone(), item_tx, emitted.clone(), events.clone()));
        let handler = tokio::spawn(handler_task(item_rx, record_tx.clone(), events.clone()));
        let journal = tokio::spawn(journal_task(journal_token.clone(), record_rx, written.clone(), events.clone()));
        Service { source_token, journal_token, source, handler, journal, _audit: record_tx, emitted, written, events }
    }

    async fn stop(self) -> (Vec<u32>, Vec<&'static str>) {
        todo!()
    }
}

#[tokio::main]
async fn main() {
    let service = Service::start();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let (written, events) = service.stop().await;
    println!("{} записей, {events:?}", written.len());
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn nothing_emitted_is_lost() {
        let service = Service::start();
        let emitted = service.emitted.clone();
        sleep(Duration::from_millis(103)).await;
        let (written, _) = service.stop().await;
        let total = emitted.load(Ordering::SeqCst);
        assert!(total > 15, "источник за 103 мс должен был выдать десятки чисел, а выдал {total}");
        assert_eq!(
            written,
            (0..total).collect::<Vec<u32>>(),
            "в журнале должно быть всё, что выдал источник ({total} чисел), по порядку: часть потерялась при остановке"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn parts_stop_in_the_right_order() {
        let service = Service::start();
        sleep(Duration::from_millis(60)).await;
        let (_, events) = service.stop().await;
        assert_eq!(
            events,
            vec!["источник остановлен", "обработчик опустошил очередь", "журнал закрыт"],
            "сначала источник, потом обработчик (доделывает очередь), последним журнал"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn the_backlog_is_drained_not_dropped() {
        let service = Service::start();
        sleep(Duration::from_millis(100)).await;
        let begin = Instant::now();
        let (written, _) = service.stop().await;
        assert!(
            begin.elapsed() >= Duration::from_millis(50),
            "в очереди ждут десятки чисел по 10 мс каждое: остановка, которая не ждёт обработчика, потеряла их ({:?})",
            begin.elapsed()
        );
        assert!(written.len() >= 20, "журнал получил всего {} записей", written.len());
    }
}
