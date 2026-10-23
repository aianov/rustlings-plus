use nfrm_rpc::{Client, Frame, FrameReader, RpcError, encode, serve};
use std::future::Future;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::time::{sleep, timeout};

async fn within<T>(what: &str, fut: impl Future<Output = T>) -> T {
    match timeout(Duration::from_secs(5), fut).await {
        Ok(v) => v,
        Err(_) => panic!("{what}: не уложились в 5 секунд, похоже на зависание"),
    }
}

async fn start_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(serve(listener, |body: Vec<u8>| async move {
        match body.split_first() {
            Some((b'S', rest)) => {
                let ms = u64::from_be_bytes(rest[..8].try_into().unwrap());
                sleep(Duration::from_millis(ms)).await;
                let mut out = b"slept:".to_vec();
                out.extend_from_slice(&rest[8..]);
                out
            }
            _ => {
                let mut out = b"echo:".to_vec();
                out.extend_from_slice(&body);
                out
            }
        }
    }));
    addr
}

fn sleepy(ms: u64, tag: &[u8]) -> Vec<u8> {
    let mut v = vec![b'S'];
    v.extend_from_slice(&ms.to_be_bytes());
    v.extend_from_slice(tag);
    v
}

#[tokio::test]
async fn one_call() {
    let addr = start_server().await;
    let client = within("connect", Client::connect(addr)).await.unwrap();
    assert_eq!(within("call", client.call(b"hi".to_vec())).await.unwrap(), b"echo:hi", "ответ сервера не совпал");
    assert_eq!(within("call", client.call(b"again".to_vec())).await.unwrap(), b"echo:again", "второй вызов по тому же соединению");
}

#[tokio::test]
async fn big_body() {
    let addr = start_server().await;
    let client = Client::connect(addr).await.unwrap();
    let body = vec![9u8; 1024 * 1024];
    let reply = within("call", client.call(body.clone())).await.unwrap();
    assert_eq!(reply.len(), body.len() + 5);
    assert!(reply[5..] == body[..], "тело искажено");
}

#[tokio::test]
async fn many_concurrent_calls_get_their_own_answers() {
    let addr = start_server().await;
    let client = Client::connect(addr).await.unwrap();
    let started = Instant::now();
    let mut tasks = Vec::new();
    for i in 0..20u64 {
        let client = client.clone();
        tasks.push(tokio::spawn(async move {
            let tag = format!("{i}");
            let reply = client.call(sleepy((20 - i) * 10, tag.as_bytes())).await.unwrap();
            (i, reply)
        }));
    }
    for t in tasks {
        let (i, reply) = within("параллельный вызов", t).await.unwrap();
        assert_eq!(reply, format!("slept:{i}").into_bytes(), "вызов {i} получил чужой ответ: ответы приходят не по порядку запросов, сопоставлять нужно по id");
    }
    assert!(
        started.elapsed() < Duration::from_millis(1200),
        "20 вызовов заняли {:?}: запросы обрабатываются по очереди, а не параллельно (при параллельной работе хватило бы около 200 мс)",
        started.elapsed()
    );
}

#[tokio::test]
async fn slow_call_does_not_block_fast_one() {
    let addr = start_server().await;
    let client = Client::connect(addr).await.unwrap();
    let slow = {
        let client = client.clone();
        tokio::spawn(async move { client.call(sleepy(600, b"slow")).await })
    };
    sleep(Duration::from_millis(50)).await;
    let started = Instant::now();
    let fast = within("быстрый вызов", client.call(b"fast".to_vec())).await.unwrap();
    assert_eq!(fast, b"echo:fast");
    assert!(started.elapsed() < Duration::from_millis(400), "быстрый вызов ждал медленного ({:?}): клиент или сервер ведёт запросы строго по очереди", started.elapsed());
    assert_eq!(within("медленный", slow).await.unwrap().unwrap(), b"slept:slow", "медленный вызов тоже должен получить свой ответ");
}

#[tokio::test]
async fn requests_go_out_without_waiting_for_replies() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (reader, mut writer) = stream.into_split();
        let mut frames = FrameReader::new(reader);
        let mut ids = Vec::new();
        for _ in 0..3 {
            ids.push(frames.next().await.unwrap().unwrap());
        }
        for f in ids.into_iter().rev() {
            writer.write_all(&encode(&Frame { id: f.id, body: f.body })).await.unwrap();
        }
        writer
    });
    let client = Client::connect(addr).await.unwrap();
    let calls: Vec<_> = (0..3u8)
        .map(|i| {
            let client = client.clone();
            tokio::spawn(async move { client.call(vec![i]).await })
        })
        .collect();
    let _writer = within("сервер получил три запроса подряд", server).await.unwrap();
    for (i, c) in calls.into_iter().enumerate() {
        assert_eq!(within("ответ", c).await.unwrap().unwrap(), vec![i as u8], "сервер ответил в обратном порядке, но каждый вызов должен получить свой ответ");
    }
}

#[tokio::test]
async fn closed_connection_fails_pending_and_new_calls() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (reader, writer) = stream.into_split();
        let mut frames = FrameReader::new(reader);
        let _ = frames.next().await;
        drop(writer);
        drop(frames);
    });
    let client = Client::connect(addr).await.unwrap();
    let res = within("зависший вызов: сервер закрылся, а ответа нет", client.call(b"x".to_vec())).await;
    assert!(matches!(res, Err(RpcError::Closed)), "соединение закрыто сервером, вызов должен получить Closed, а не висеть: {res:?}");
    sleep(Duration::from_millis(50)).await;
    let res = within("вызов после закрытия", client.call(b"y".to_vec())).await;
    assert!(res.is_err(), "вызов по закрытому соединению должен сразу вернуть ошибку");
}

#[tokio::test]
async fn connect_to_nobody_fails() {
    let dead = TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap();
    assert!(Client::connect(dead).await.is_err(), "порт никто не слушает");
}

#[tokio::test]
async fn cancelled_calls_do_not_leak() {
    let silent = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = silent.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut s, _) = silent.accept().await.unwrap();
        let mut sink = [0u8; 4096];
        use tokio::io::AsyncReadExt;
        while s.read(&mut sink).await.map(|n| n > 0).unwrap_or(false) {}
    });
    let client = Client::connect(addr).await.unwrap();
    for _ in 0..100 {
        let res = timeout(Duration::from_millis(5), client.call(b"x".to_vec())).await;
        assert!(res.is_err(), "сервер молчит, вызов обязан не закончиться сам");
    }
    assert_eq!(client.pending(), 0, "после 100 отменённых вызовов в таблице осталось {}: запись должна убираться и тогда, когда future выбросили", client.pending());
}
