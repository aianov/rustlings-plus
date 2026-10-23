
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

fn main() {}

#[derive(Debug, PartialEq, Eq)]
pub struct Overloaded {
    pub retry_after: Duration,
}

pub struct Shedder {
    max_in_flight: usize,
    retry_after: Duration,
    in_flight: Arc<AtomicUsize>,
}

/// Место в работе. Пока оно живо, счётчик занят.
pub struct Admit {
    counter: Arc<AtomicUsize>,
}

impl Drop for Admit {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Shedder {
    pub fn new(max_in_flight: usize, retry_after: Duration) -> Shedder {
        Shedder { max_in_flight, retry_after, in_flight: Arc::new(AtomicUsize::new(0)) }
    }

    pub fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::SeqCst)
    }

    /// Занимает место или отказывает. Сколько бы потоков ни стучалось одновременно, мест выдано не больше лимита.
    pub fn try_admit(&self) -> Result<Admit, Overloaded> {
        self.in_flight
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| (n < self.max_in_flight).then_some(n + 1))
            .map(|_| Admit { counter: self.in_flight.clone() })
            .map_err(|_| Overloaded { retry_after: self.retry_after })
    }
}

/// Ответ клиенту: `503 Service Unavailable`, `Retry-After` в целых секундах (округление вверх, не меньше одной),
/// `Connection: close`, `Content-Length: 0`.
pub fn overloaded_response(o: &Overloaded) -> Vec<u8> {
    let secs = (o.retry_after.as_secs_f64().ceil() as u64).max(1);
    format!("HTTP/1.1 503 Service Unavailable\r\nRetry-After: {secs}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").into_bytes()
}

/// Принимает соединения. Пока места есть, соединение уходит в `handler` (место освобождается, когда обработчик закончил).
/// Когда мест нет, клиент получает `overloaded_response`, соединение закрывается, и цикл приёма сразу идёт дальше.
pub async fn serve<H, Fut>(listener: TcpListener, shedder: Arc<Shedder>, handler: H)
where
    H: Fn(TcpStream) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    loop {
        let Ok((mut stream, _)) = listener.accept().await else {
            tokio::time::sleep(Duration::from_millis(10)).await;
            continue;
        };
        match shedder.try_admit() {
            Ok(admit) => {
                let work = handler(stream);
                tokio::spawn(async move {
                    work.await;
                    drop(admit);
                });
            }
            Err(overloaded) => {
                tokio::spawn(async move {
                    let _ = stream.write_all(&overloaded_response(&overloaded)).await;
                    let _ = stream.shutdown().await;
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;
    use tokio::time::timeout;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn admits_up_to_the_limit() {
        let s = Shedder::new(3, MS(1500));
        let a = s.try_admit().unwrap();
        let _b = s.try_admit().unwrap();
        let _c = s.try_admit().unwrap();
        assert_eq!(s.in_flight(), 3);
        assert_eq!(s.try_admit().err(), Some(Overloaded { retry_after: MS(1500) }), "четвёртый запрос сверх лимита три");
        drop(a);
        assert_eq!(s.in_flight(), 2, "освобождённое место должно вернуться в счёт");
        assert!(s.try_admit().is_ok());
    }

    #[test]
    fn never_admits_more_than_the_limit_under_a_race() {
        let s = Arc::new(Shedder::new(50, MS(1000)));
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let admitted: usize = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    let s = s.clone();
                    let barrier = barrier.clone();
                    scope.spawn(move || {
                        barrier.wait();
                        (0..100).filter_map(|_| s.try_admit().ok()).collect::<Vec<_>>()
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap().len()).sum()
        });
        assert_eq!(admitted, 50, "из 800 попыток одновременно выдано {admitted} мест при лимите 50");
        assert_eq!(s.in_flight(), 0, "все места освобождены, счёт обязан вернуться к нулю");
    }

    #[test]
    fn the_response_is_a_valid_503_with_retry_after() {
        let text = String::from_utf8(overloaded_response(&Overloaded { retry_after: MS(1500) })).unwrap();
        assert!(text.starts_with("HTTP/1.1 503 Service Unavailable\r\n"), "ответ должен начинаться со строки статуса 503: {text:?}");
        assert!(text.contains("\r\nRetry-After: 2\r\n"), "1500 мс округляются вверх до 2 секунд: {text:?}");
        assert!(text.contains("\r\nConnection: close\r\n"), "соединение закрываем: {text:?}");
        assert!(text.contains("\r\nContent-Length: 0\r\n"), "тела нет, длину нужно сказать: {text:?}");
        assert!(text.ends_with("\r\n\r\n"), "заголовки заканчиваются пустой строкой");
        let tiny = String::from_utf8(overloaded_response(&Overloaded { retry_after: MS(100) })).unwrap();
        assert!(tiny.contains("Retry-After: 1\r\n"), "Retry-After: 0 не имеет смысла, минимум секунда: {tiny:?}");
    }

    async fn start(limit: usize) -> (std::net::SocketAddr, Arc<Shedder>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let shedder = Arc::new(Shedder::new(limit, Duration::from_secs(2)));
        tokio::spawn(serve(listener, shedder.clone(), |mut s: TcpStream| async move {
            let mut b = [0u8; 1];
            let _ = s.read(&mut b).await;
        }));
        (addr, shedder)
    }

    async fn is_rejected(stream: &mut TcpStream) -> bool {
        let mut buf = Vec::new();
        match timeout(Duration::from_secs(3), stream.read_to_end(&mut buf)).await {
            Ok(_) => String::from_utf8_lossy(&buf).starts_with("HTTP/1.1 503"),
            Err(_) => false,
        }
    }

    async fn is_held(stream: &mut TcpStream) -> bool {
        let mut b = [0u8; 1];
        timeout(MS(200), stream.read(&mut b)).await.is_err()
    }

    #[tokio::test]
    async fn connections_over_the_limit_get_503_at_once() {
        let (addr, shedder) = start(2).await;
        let mut a = TcpStream::connect(addr).await.unwrap();
        let mut b = TcpStream::connect(addr).await.unwrap();
        assert!(is_held(&mut a).await && is_held(&mut b).await, "первые два соединения должны быть приняты в работу");
        let mut third = TcpStream::connect(addr).await.unwrap();
        assert!(is_rejected(&mut third).await, "третье соединение при лимите два обязано получить 503 и закрытие");
        assert_eq!(shedder.in_flight(), 2, "отказанное соединение не занимает место");
    }

    #[tokio::test]
    async fn a_crowd_of_rejections_does_not_block_the_accept_loop() {
        let (addr, shedder) = start(1).await;
        let mut held = TcpStream::connect(addr).await.unwrap();
        assert!(is_held(&mut held).await);
        let mut crowd = Vec::new();
        for _ in 0..30 {
            crowd.push(TcpStream::connect(addr).await.unwrap());
        }
        for stream in &mut crowd {
            assert!(is_rejected(stream).await, "каждое соединение сверх лимита должно получить 503");
        }
        assert_eq!(shedder.in_flight(), 1);
    }

    #[tokio::test]
    async fn a_place_is_reused_after_the_handler_finishes() {
        let (addr, shedder) = start(1).await;
        let first = TcpStream::connect(addr).await.unwrap();
        timeout(Duration::from_secs(2), async {
            while shedder.in_flight() < 1 {
                tokio::time::sleep(MS(5)).await;
            }
        })
        .await
        .expect("обработчик первого соединения не запустился");
        drop(first);
        timeout(Duration::from_secs(2), async {
            while shedder.in_flight() > 0 {
                tokio::time::sleep(MS(5)).await;
            }
        })
        .await
        .expect("место не вернулось после закрытия соединения клиентом");
        let mut again = TcpStream::connect(addr).await.unwrap();
        assert!(is_held(&mut again).await, "после освобождения места новое соединение обслуживается, а не получает 503");
    }
}
