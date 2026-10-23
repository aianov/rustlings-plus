// Без лимита сервер примет столько клиентов, сколько выдержит память и дескрипторы, и умрёт
// от тысячи медленных. Ограничить число соединений мало: пока мест нет, лишние клиенты не
// должны занимать у сервера ничего, кроме очереди в ядре. Тест следит и за этим.
// TODO: Допиши serve_limited: не больше max соединений одновременно, остальные обслуживаются
// по мере освобождения мест. Приёмник обобщён трейтом Accept, чтобы тест мог считать вызовы.

use std::future::Future;
use std::io;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;

trait Accept {
    type Conn;
    fn accept(&mut self) -> impl Future<Output = io::Result<Self::Conn>> + Send;
}

impl Accept for TcpListener {
    type Conn = TcpStream;

    async fn accept(&mut self) -> io::Result<TcpStream> {
        Ok(TcpListener::accept(self).await?.0)
    }
}

async fn serve_limited<A, F, Fut>(mut acceptor: A, max: usize, handler: F)
where
    A: Accept + Send + 'static,
    A::Conn: Send + 'static,
    F: Fn(A::Conn) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    todo!()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    println!("слушаем {}", listener.local_addr()?);
    serve_limited(listener, 2, |mut stream: TcpStream| async move {
        let _ = stream.write_all(b"hello\n").await;
    })
    .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use std::sync::Mutex;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;
    use tokio::time::{sleep, timeout};

    async fn run(max: usize, clients: usize) -> (usize, Vec<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let (a, p) = (active.clone(), peak.clone());
        tokio::spawn(serve_limited(listener, max, move |mut stream| {
            let (a, p) = (a.clone(), p.clone());
            async move {
                let now = a.fetch_add(1, SeqCst) + 1;
                p.fetch_max(now, SeqCst);
                sleep(Duration::from_millis(80)).await;
                let _ = stream.write_all(b"done").await;
                a.fetch_sub(1, SeqCst);
            }
        }));
        let mut tasks = Vec::new();
        for _ in 0..clients {
            tasks.push(tokio::spawn(async move {
                let mut s = TcpStream::connect(addr).await.unwrap();
                let mut out = String::new();
                s.read_to_string(&mut out).await.unwrap();
                out
            }));
        }
        let mut replies = Vec::new();
        for t in tasks {
            replies.push(timeout(Duration::from_secs(8), t).await.expect("клиент не дождался ответа: лишние соединения должны обслуживаться по мере освобождения мест").unwrap());
        }
        (peak.load(SeqCst), replies)
    }

    #[tokio::test]
    async fn never_exceeds_the_limit() {
        let (peak, _) = run(2, 6).await;
        assert!(peak <= 2, "одновременно работало {peak} соединений при лимите 2: разрешение надо брать до accept и держать до конца handler");
    }

    #[tokio::test]
    async fn everyone_is_served_eventually() {
        let (peak, replies) = run(2, 6).await;
        assert_eq!(replies.len(), 6, "ответили не все клиенты");
        assert!(replies.iter().all(|r| r == "done"), "ответы должны быть done: {replies:?}");
        assert_eq!(peak, 2, "лимит 2 не использован полностью (пик {peak}): лишние клиенты должны ждать места, а не отваливаться");
    }

    #[tokio::test]
    async fn limit_of_one_serializes() {
        let (peak, replies) = run(1, 3).await;
        assert_eq!(peak, 1, "при лимите 1 соединения идут строго по очереди");
        assert_eq!(replies.len(), 3);
    }

    struct Counting {
        queue: VecDeque<u32>,
        accepts: Arc<AtomicUsize>,
    }

    impl Accept for Counting {
        type Conn = u32;

        async fn accept(&mut self) -> io::Result<u32> {
            self.accepts.fetch_add(1, SeqCst);
            match self.queue.pop_front() {
                Some(id) => Ok(id),
                None => std::future::pending().await,
            }
        }
    }

    #[tokio::test]
    async fn does_not_accept_while_full() {
        let accepts = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(Mutex::new(Vec::new()));
        let gate = Arc::new(Semaphore::new(0));
        let acceptor = Counting { queue: (0..3).collect(), accepts: accepts.clone() };
        let (s, g) = (started.clone(), gate.clone());
        let server = tokio::spawn(serve_limited(acceptor, 1, move |id| {
            let (s, g) = (s.clone(), g.clone());
            async move {
                s.lock().unwrap().push(id);
                g.acquire().await.unwrap().forget();
            }
        }));
        sleep(Duration::from_millis(150)).await;
        assert_eq!(started.lock().unwrap().len(), 1, "при лимите 1 должно работать ровно одно соединение");
        assert_eq!(
            accepts.load(SeqCst),
            1,
            "пока единственное место занято, сервер уже позвал accept ещё раз: разрешение надо брать ДО accept, иначе сокет принят и держит ресурсы впустую"
        );
        gate.add_permits(1);
        sleep(Duration::from_millis(150)).await;
        assert_eq!(started.lock().unwrap().clone(), vec![0, 1], "после освобождения места должно стартовать следующее соединение, по порядку");
        assert_eq!(accepts.load(SeqCst), 2, "второй accept должен случиться только после освобождения места");
        gate.add_permits(10);
        timeout(Duration::from_secs(3), async {
            while started.lock().unwrap().len() < 3 {
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("третье соединение так и не стартовало");
        server.abort();
    }
}
