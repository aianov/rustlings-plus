use ntsv_proxy::{Totals, run};
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};

const WAIT: Duration = Duration::from_secs(5);

async fn within<T>(what: &str, fut: impl Future<Output = T>) -> T {
    match timeout(WAIT, fut).await {
        Ok(v) => v,
        Err(_) => panic!("{what}: не уложились в 5 секунд, похоже на взаимное ожидание"),
    }
}

struct Proxy {
    addr: SocketAddr,
    totals: Arc<Totals>,
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<io::Result<()>>,
}

async fn start_proxy(upstream: SocketAddr) -> Proxy {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let totals = Arc::new(Totals::default());
    let (tx, rx) = oneshot::channel::<()>();
    let task = tokio::spawn(run(listener, upstream, totals.clone(), async move {
        let _ = rx.await;
    }));
    Proxy { addr, totals, stop: Some(tx), task }
}

async fn upstream_listener() -> (TcpListener, SocketAddr) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let a = l.local_addr().unwrap();
    (l, a)
}

async fn echo_upstream() -> SocketAddr {
    let (l, addr) = upstream_listener().await;
    tokio::spawn(async move {
        loop {
            let (mut s, _) = l.accept().await.unwrap();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                loop {
                    let n = s.read(&mut buf).await.unwrap_or(0);
                    if n == 0 || s.write_all(&buf[..n]).await.is_err() {
                        let _ = s.shutdown().await;
                        return;
                    }
                }
            });
        }
    });
    addr
}

async fn wait_for(what: &str, cond: impl Fn() -> bool) {
    within(what, async {
        while !cond() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
}

#[tokio::test]
async fn bytes_flow_both_ways() {
    let (l, up_addr) = upstream_listener().await;
    tokio::spawn(async move {
        let (mut s, _) = l.accept().await.unwrap();
        let mut b = [0u8; 4];
        s.read_exact(&mut b).await.unwrap();
        assert_eq!(&b, b"ping");
        s.write_all(b"pong").await.unwrap();
    });
    let proxy = start_proxy(up_addr).await;
    let mut client = TcpStream::connect(proxy.addr).await.unwrap();
    client.write_all(b"ping").await.unwrap();
    let mut b = [0u8; 4];
    within("ответ upstream", client.read_exact(&mut b)).await.unwrap();
    assert_eq!(&b, b"pong", "ответ upstream должен дойти до клиента без изменений");
}

#[tokio::test]
async fn client_half_close_reaches_upstream() {
    let (l, up_addr) = upstream_listener().await;
    tokio::spawn(async move {
        let (mut s, _) = l.accept().await.unwrap();
        let mut request = Vec::new();
        s.read_to_end(&mut request).await.unwrap();
        s.write_all(format!("got {}", request.len()).as_bytes()).await.unwrap();
    });
    let proxy = start_proxy(up_addr).await;
    let mut client = TcpStream::connect(proxy.addr).await.unwrap();
    client.write_all(&vec![7u8; 100_000]).await.unwrap();
    client.shutdown().await.unwrap();
    let mut reply = String::new();
    within(
        "ответ после half-close",
        client.read_to_string(&mut reply),
    )
    .await
    .expect("upstream не дождался EOF: прокси должен передать закрытие записи клиента дальше (shutdown), а не молчать");
    assert_eq!(reply, "got 100000", "upstream должен был получить весь запрос до EOF");
    wait_for("счётчик up", || proxy.totals.up.load(Relaxed) == 100_000).await;
}

#[tokio::test]
async fn upstream_half_close_reaches_client_and_other_direction_keeps_working() {
    let (l, up_addr) = upstream_listener().await;
    let (got_tx, got_rx) = oneshot::channel::<Vec<u8>>();
    tokio::spawn(async move {
        let (mut s, _) = l.accept().await.unwrap();
        s.write_all(b"hello").await.unwrap();
        s.shutdown().await.unwrap();
        let mut rest = Vec::new();
        s.read_to_end(&mut rest).await.unwrap();
        let _ = got_tx.send(rest);
    });
    let proxy = start_proxy(up_addr).await;
    let mut client = TcpStream::connect(proxy.addr).await.unwrap();
    let mut first = Vec::new();
    within("EOF от upstream", client.read_to_end(&mut first))
        .await
        .expect("клиент должен увидеть EOF, когда upstream закрыл свою запись: проброс half-close в обратную сторону");
    assert_eq!(first, b"hello");
    client.write_all(b"late data").await.unwrap();
    client.shutdown().await.unwrap();
    let got = within("поздние данные", got_rx).await.unwrap();
    assert_eq!(got, b"late data", "после half-close в одну сторону другая должна продолжать работать: нельзя закрывать обе половины сразу");
}

#[tokio::test]
async fn refused_upstream_closes_the_client_and_is_counted() {
    let (l, dead) = upstream_listener().await;
    drop(l);
    let proxy = start_proxy(dead).await;
    let mut client = TcpStream::connect(proxy.addr).await.unwrap();
    let mut rest = Vec::new();
    let res = within("закрытие клиента", client.read_to_end(&mut rest)).await;
    assert!(res.is_err() || rest.is_empty(), "upstream недоступен, клиенту нечего получать");
    wait_for("счётчик failed", || proxy.totals.failed.load(Relaxed) == 1).await;
    assert_eq!(proxy.totals.connections.load(Relaxed), 0, "соединение с upstream не состоялось, в connections его быть не должно");
}

#[tokio::test]
async fn counts_exact_bytes_for_a_megabyte() {
    let up = echo_upstream().await;
    let proxy = start_proxy(up).await;
    let client = TcpStream::connect(proxy.addr).await.unwrap();
    let (mut rd, mut wr) = client.into_split();
    let data: Vec<u8> = (0..1024 * 1024).map(|i| (i % 253) as u8).collect();
    let expected = data.clone();
    let writer = tokio::spawn(async move {
        wr.write_all(&data).await.unwrap();
        wr.shutdown().await.unwrap();
    });
    let mut back = Vec::new();
    within("мегабайт через прокси", rd.read_to_end(&mut back)).await.unwrap();
    writer.await.unwrap();
    assert!(back == expected, "мегабайт вернулся искажённым или не целиком ({} байт)", back.len());
    wait_for("счётчики", || proxy.totals.up.load(Relaxed) == 1_048_576 && proxy.totals.down.load(Relaxed) == 1_048_576).await;
    assert_eq!(proxy.totals.connections.load(Relaxed), 1, "одно успешное соединение");
}

#[tokio::test]
async fn many_clients_in_parallel() {
    let up = echo_upstream().await;
    let proxy = start_proxy(up).await;
    let mut tasks = Vec::new();
    for i in 0..20u8 {
        let addr = proxy.addr;
        tasks.push(tokio::spawn(async move {
            let mut c = TcpStream::connect(addr).await.unwrap();
            c.write_all(&[i; 500]).await.unwrap();
            c.shutdown().await.unwrap();
            let mut back = Vec::new();
            c.read_to_end(&mut back).await.unwrap();
            assert_eq!(back, vec![i; 500], "клиент {i} получил чужие или испорченные байты");
        }));
    }
    for t in tasks {
        within("параллельный клиент", t).await.unwrap();
    }
    wait_for("connections", || proxy.totals.connections.load(Relaxed) == 20).await;
}

#[tokio::test]
async fn upstream_reset_does_not_leave_the_client_hanging() {
    let (l, up_addr) = upstream_listener().await;
    tokio::spawn(async move {
        let (s, _) = l.accept().await.unwrap();
        s.readable().await.unwrap();
        drop(s);
    });
    let proxy = start_proxy(up_addr).await;
    let mut client = TcpStream::connect(proxy.addr).await.unwrap();
    client.write_all(b"x").await.unwrap();
    let mut rest = Vec::new();
    let _ = within(
        "клиент после сброса upstream",
        client.read_to_end(&mut rest),
    )
    .await;
}

#[tokio::test]
async fn shutdown_stops_the_listener() {
    let up = echo_upstream().await;
    let mut proxy = start_proxy(up).await;
    let _ = proxy.stop.take().unwrap().send(());
    within("остановка", proxy.task).await.unwrap().unwrap();
    assert!(TcpStream::connect(proxy.addr).await.is_err(), "после остановки порт должен быть закрыт");
}
