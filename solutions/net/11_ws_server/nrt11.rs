// Клиент открыл ленту, и приложение отправило одновременно пять запросов: профиль, посты, тренды, непрочитанные, настройки.
// Если сервер разбирает запросы соединения по одному, лента ждёт сумму всех задержек, а один медленный запрос запирает остальные.
// Нужно обрабатывать запросы одного соединения параллельно, но с потолком: десять тысяч запросов от одного клиента не должны
// породить десять тысяч задач. Ответы уходят, как только готовы (не по порядку), каждый со своим id.
// TODO: Допиши serve_requests. Паника одного обработчика не должна ломать ни соединение, ни лимит.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinSet;

struct Request {
    id: u64,
    method: String,
}

type Reply = (u64, Result<String, String>);
type Handler = Arc<dyn Fn(Request) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> + Send + Sync>;

async fn serve_requests(handler: Handler, mut inbound: mpsc::Receiver<Request>, outbound: mpsc::Sender<Reply>, max_in_flight: usize) {
    let permits = Arc::new(Semaphore::new(max_in_flight));
    let mut running = JoinSet::new();
    loop {
        let permit = permits.clone().acquire_owned().await.expect("семафор не закрывается");
        let Some(request) = inbound.recv().await else {
            break;
        };
        let id = request.id;
        let work = tokio::spawn(handler(request));
        let reply_to = outbound.clone();
        running.spawn(async move {
            let result = work.await.unwrap_or_else(|_| Err("internal".to_string()));
            let _ = reply_to.send((id, result)).await;
            drop(permit);
        });
        if outbound.is_closed() {
            break;
        }
    }
    while running.join_next().await.is_some() {}
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::time::{Instant, sleep, timeout};

    #[derive(Default)]
    struct Gauge {
        now: AtomicUsize,
        peak: AtomicUsize,
    }

    fn handler(gauge: Arc<Gauge>) -> Handler {
        Arc::new(move |request: Request| {
            let gauge = gauge.clone();
            Box::pin(async move {
                let running = gauge.now.fetch_add(1, Ordering::SeqCst) + 1;
                gauge.peak.fetch_max(running, Ordering::SeqCst);
                let result = if let Some(ms) = request.method.strip_prefix("sleep:") {
                    sleep(Duration::from_millis(ms.parse().unwrap())).await;
                    Ok(format!("done {}", request.id))
                } else if request.method == "panic" {
                    gauge.now.fetch_sub(1, Ordering::SeqCst);
                    panic!("обработчик упал");
                } else {
                    Err("unknown".to_string())
                };
                gauge.now.fetch_sub(1, Ordering::SeqCst);
                result
            })
        })
    }

    fn req(id: u64, method: &str) -> Request {
        Request { id, method: method.to_string() }
    }

    struct Run {
        inbound: mpsc::Sender<Request>,
        replies: mpsc::Receiver<Reply>,
        server: tokio::task::JoinHandle<()>,
        gauge: Arc<Gauge>,
        began: Instant,
    }

    fn run(limit: usize) -> Run {
        let gauge = Arc::new(Gauge::default());
        let (inbound, rx) = mpsc::channel(64);
        let (tx, replies) = mpsc::channel(64);
        let server = tokio::spawn(serve_requests(handler(gauge.clone()), rx, tx, limit));
        Run { inbound, replies, server, gauge, began: Instant::now() }
    }

    impl Run {
        async fn collect(&mut self, count: usize) -> Vec<(u64, Result<String, String>, Duration)> {
            let mut out = Vec::new();
            while out.len() < count {
                let (id, result) = timeout(Duration::from_secs(30), self.replies.recv()).await.expect("ответы перестали приходить").expect("канал ответов закрылся раньше времени");
                out.push((id, result, self.began.elapsed()));
            }
            out
        }
    }

    #[tokio::test(start_paused = true)]
    async fn requests_run_in_parallel_and_replies_come_in_completion_order() {
        let mut run = run(10);
        run.inbound.send(req(1, "sleep:500")).await.unwrap();
        run.inbound.send(req(2, "sleep:10")).await.unwrap();
        run.inbound.send(req(3, "sleep:200")).await.unwrap();
        let replies = run.collect(3).await;
        let order: Vec<u64> = replies.iter().map(|r| r.0).collect();
        assert_eq!(order, vec![2, 3, 1], "ответы должны уходить по готовности, а не по порядку запросов");
        assert!(replies[2].2 < Duration::from_millis(600), "три запроса заняли {:?}: они выполнялись по очереди, а не вместе", replies[2].2);
        assert_eq!(replies[0].1, Ok("done 2".to_string()), "ответ должен нести id своего запроса");
    }

    #[tokio::test(start_paused = true)]
    async fn in_flight_limit_is_respected_and_used() {
        let mut run = run(2);
        for id in 0..6 {
            run.inbound.send(req(id, "sleep:100")).await.unwrap();
        }
        let replies = run.collect(6).await;
        assert_eq!(run.gauge.peak.load(Ordering::SeqCst), 2, "одновременно должны работать ровно два обработчика: потолок не превышен и используется полностью");
        let last = replies[5].2;
        assert!(last >= Duration::from_millis(300) && last < Duration::from_millis(400), "шесть запросов по 100 мс при лимите 2 это три волны: {last:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn closing_the_inbound_flushes_everything_in_flight() {
        let mut run = run(4);
        for id in 0..5 {
            run.inbound.send(req(id, "sleep:50")).await.unwrap();
        }
        drop(std::mem::replace(&mut run.inbound, mpsc::channel(1).0));
        let replies = run.collect(5).await;
        assert_eq!(replies.len(), 5, "после закрытия входа все принятые запросы должны получить ответ");
        timeout(Duration::from_secs(5), run.server).await.expect("serve_requests не завершился после ответов").unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn panicking_handler_gets_an_error_reply_and_keeps_its_permit_free() {
        let mut run = run(1);
        run.inbound.send(req(1, "panic")).await.unwrap();
        for id in 2..5 {
            run.inbound.send(req(id, "sleep:10")).await.unwrap();
        }
        let replies = run.collect(4).await;
        let first = replies.iter().find(|r| r.0 == 1).expect("на запрос с паникой нет ответа");
        assert!(first.1.is_err(), "упавший обработчик должен дать ответ-ошибку со своим id");
        assert_eq!(replies.iter().filter(|r| r.1.is_ok()).count(), 3, "остальные запросы выполняются как обычно (если разрешение утекло, они зависнут)");
    }

    #[tokio::test(start_paused = true)]
    async fn dead_client_stops_the_loop() {
        let mut run = run(2);
        run.inbound.send(req(1, "sleep:10")).await.unwrap();
        let _ = run.collect(1).await;
        drop(run.replies);
        for id in 2..10 {
            if run.inbound.send(req(id, "sleep:10")).await.is_err() {
                break;
            }
        }
        timeout(Duration::from_secs(5), run.server).await.expect("клиент пропал (канал ответов закрыт), а цикл всё ещё работает").unwrap();
    }
}
