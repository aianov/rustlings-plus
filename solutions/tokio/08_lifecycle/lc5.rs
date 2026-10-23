use futures::future::select_all;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use std::sync::Mutex;
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

#[derive(Clone)]
struct Tracker {
    running: Arc<watch::Sender<usize>>,
}

struct Exit(Arc<watch::Sender<usize>>);

impl Drop for Exit {
    fn drop(&mut self) {
        self.0.send_modify(|count| *count -= 1);
    }
}

#[allow(dead_code)]
impl Tracker {
    fn new() -> Tracker {
        Tracker { running: Arc::new(watch::channel(0).0) }
    }

    fn spawn<F>(&self, task: F) -> JoinHandle<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.running.send_modify(|count| *count += 1);
        let exit = Exit(self.running.clone());
        tokio::spawn(async move {
            let _exit = exit;
            task.await;
        })
    }

    fn len(&self) -> usize {
        *self.running.borrow()
    }

    async fn wait(&self) {
        let mut rx = self.running.subscribe();
        let _ = rx.wait_for(|count| *count == 0).await;
    }
}

async fn serve(mut jobs: mpsc::Receiver<u32>, token: Token, done: Arc<Mutex<Vec<u32>>>) {
    let tracker = Tracker::new();
    loop {
        tokio::select! {
            biased;
            _ = token.cancelled() => break,
            job = jobs.recv() => match job {
                Some(job) => {
                    let done = done.clone();
                    tracker.spawn(async move {
                        sleep(Duration::from_millis(100)).await;
                        done.lock().unwrap().push(job);
                    });
                }
                None => break,
            },
        }
    }
    tracker.wait().await;
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel(8);
    let done = Arc::new(Mutex::new(Vec::new()));
    tx.send(1).await.unwrap();
    drop(tx);
    serve(rx, Token::new(), done.clone()).await;
    println!("{:?}", done.lock().unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn in_flight_jobs_finish_and_new_ones_are_not_taken() {
        let (tx, rx) = mpsc::channel(8);
        let token = Token::new();
        let done = Arc::new(Mutex::new(Vec::new()));
        let begin = Instant::now();
        let server = tokio::spawn(serve(rx, token.clone(), done.clone()));
        for job in 1..=3 {
            tx.send(job).await.unwrap();
        }
        sleep(Duration::from_millis(50)).await;
        token.cancel();
        tx.send(4).await.expect("очередь жива, пока сервер не вернулся");
        server.await.unwrap();
        assert_eq!(begin.elapsed(), Duration::from_millis(100), "сервер ждёт доделки трёх заданий, ровно сто миллисекунд");
        let mut got = done.lock().unwrap().clone();
        got.sort();
        assert_eq!(got, vec![1, 2, 3], "три взятых задания доделаны, четвёртое после сигнала брать нельзя");
    }

    #[tokio::test(start_paused = true)]
    async fn a_closed_queue_ends_the_server_after_the_last_job() {
        let (tx, rx) = mpsc::channel(8);
        let done = Arc::new(Mutex::new(Vec::new()));
        tx.send(1).await.unwrap();
        tx.send(2).await.unwrap();
        drop(tx);
        let begin = Instant::now();
        serve(rx, Token::new(), done.clone()).await;
        assert_eq!(begin.elapsed(), Duration::from_millis(100), "оба задания идут одновременно: сто миллисекунд");
        assert_eq!(done.lock().unwrap().len(), 2, "оба задания доделаны, прежде чем сервер вернулся");
    }

    #[tokio::test(start_paused = true)]
    async fn a_cancelled_token_takes_nothing() {
        let (tx, rx) = mpsc::channel(8);
        let token = Token::new();
        token.cancel();
        let done = Arc::new(Mutex::new(Vec::new()));
        tx.send(1).await.unwrap();
        tx.send(2).await.unwrap();
        let begin = Instant::now();
        serve(rx, token, done.clone()).await;
        assert_eq!(begin.elapsed(), Duration::ZERO, "отмена до старта: возвращаемся сразу");
        assert!(done.lock().unwrap().is_empty(), "сигнал уже был, брать задания из очереди нельзя");
    }
}
