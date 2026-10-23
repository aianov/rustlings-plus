// В контейнере с минимальным образом нет `curl`, а Docker всё равно хочет знать, жив ли сервис: HEALTHCHECK запускает команду и смотрит на код выхода.
// Ставить curl ради проверки значит раздувать образ и расширять поверхность атаки. Свой маленький бинарь на Rust делает то же: открывает TCP-соединение,
// шлёт `GET /ready` и смотрит на статус. Главное, чтобы проверка сама никогда не зависала и не врала.
// Цель: `check` возвращает 0, если сервис ответил статусом 2xx, и 1 во всех остальных случаях (ошибка соединения, молчание дольше `timeout`
// на всё про всё, мусор вместо ответа, другой статус). Запрос корректный: `GET <путь> HTTP/1.1`, заголовки `Host` и `Connection: close`.


use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn main() {}

pub async fn check(addr: SocketAddr, path: &str, timeout: Duration) -> u8 {
    let _ = (addr, path, timeout);
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;
    use tokio::net::TcpListener;

    const T: Duration = Duration::from_millis(400);

    /// Сервер, который отвечает заданными кусками с паузами и запоминает запрос.
    async fn server(chunks: Vec<(&'static [u8], u64)>) -> (SocketAddr, Arc<Mutex<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(String::new()));
        let log = seen.clone();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = listener.accept().await {
                let chunks = chunks.clone();
                let log = log.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 2048];
                    let n = s.read(&mut buf).await.unwrap_or(0);
                    *log.lock().unwrap() = String::from_utf8_lossy(&buf[..n]).to_string();
                    for (data, pause) in chunks {
                        tokio::time::sleep(Duration::from_millis(pause)).await;
                        if s.write_all(data).await.is_err() {
                            return;
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(30)).await;
                });
            }
        });
        (addr, seen)
    }

    #[tokio::test]
    async fn success_statuses_are_healthy() {
        for line in [&b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n"[..], b"HTTP/1.1 204 No Content\r\n\r\n", b"HTTP/1.0 200 OK\r\n\r\n"] {
            let (addr, _) = server(vec![(line, 0)]).await;
            assert_eq!(check(addr, "/ready", T).await, 0, "{:?}", String::from_utf8_lossy(line));
        }
    }

    #[tokio::test]
    async fn other_statuses_are_unhealthy() {
        for line in [&b"HTTP/1.1 503 Service Unavailable\r\n\r\n"[..], b"HTTP/1.1 301 Moved\r\nLocation: /\r\n\r\n", b"HTTP/1.1 404 Not Found\r\n\r\n", b"HTTP/1.1 500 Oops\r\n\r\n"] {
            let (addr, _) = server(vec![(line, 0)]).await;
            assert_eq!(check(addr, "/ready", T).await, 1, "{:?}", String::from_utf8_lossy(line));
        }
    }

    #[tokio::test]
    async fn the_request_is_a_proper_http_request() {
        let (addr, seen) = server(vec![(b"HTTP/1.1 200 OK\r\n\r\n", 0)]).await;
        assert_eq!(check(addr, "/ready", T).await, 0);
        let req = seen.lock().unwrap().clone();
        assert!(req.starts_with("GET /ready HTTP/1.1\r\n"), "{req:?}");
        assert!(req.contains("\r\nHost: ") && req.contains("\r\nConnection: close\r\n") && req.ends_with("\r\n\r\n"), "{req:?}");
    }

    #[tokio::test]
    async fn a_refused_connection_is_unhealthy() {
        let addr = {
            let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
            l.local_addr().unwrap()
        };
        assert_eq!(check(addr, "/ready", T).await, 1);
    }

    #[tokio::test]
    async fn silence_does_not_hang_the_check() {
        let (addr, _) = server(vec![]).await;
        let start = Instant::now();
        assert_eq!(check(addr, "/ready", Duration::from_millis(300)).await, 1);
        assert!(start.elapsed() < Duration::from_millis(1500), "проверка ждала {:?}: таймаут должен охватывать всё", start.elapsed());
    }

    #[tokio::test]
    async fn a_slow_trickle_is_cut_by_the_overall_timeout() {
        let drip: Vec<(&'static [u8], u64)> = (0..50).map(|_| (&b"H"[..], 100)).collect();
        let (addr, _) = server(drip).await;
        let start = Instant::now();
        assert_eq!(check(addr, "/ready", Duration::from_millis(300)).await, 1);
        assert!(start.elapsed() < Duration::from_millis(1500), "по байту в 100 мс держит проверку {:?}: таймаут должен быть общим, а не на каждое чтение", start.elapsed());
    }

    #[tokio::test]
    async fn garbage_and_split_answers() {
        let (addr, _) = server(vec![(b"BANANA\r\n\r\n", 0)]).await;
        assert_eq!(check(addr, "/ready", T).await, 1, "ответ не HTTP");
        let (addr, _) = server(vec![(b"HTT", 0), (b"P/1.1 20", 30), (b"0 OK\r\n\r\n", 30)]).await;
        assert_eq!(check(addr, "/ready", T).await, 0, "ответ пришёл несколькими кусками");
        let (addr, _) = server(vec![(b"HTTP/1.1 abc OK\r\n\r\n", 0)]).await;
        assert_eq!(check(addr, "/ready", T).await, 1, "статус не число");
    }
}
