use std::time::Duration;

use bytes::BytesMut;
use framing::{Decoder, Encoder};
use resp_client::{Client, RedisConfig, RespCodec, RespError, Value, cmd, key, scan_keys};

fn decoded(raw: &[u8]) -> Result<Option<Value>, RespError> {
    let mut buf = BytesMut::from(raw);
    RespCodec::default().decode(&mut buf)
}

#[test]
fn codec_reads_whole_frames_and_waits_for_the_rest() {
    let raw = b"*3\r\n$3\r\nfoo\r\n:42\r\n*2\r\n+OK\r\n$-1\r\n";
    for cut in 0..raw.len() {
        assert_eq!(
            decoded(&raw[..cut]).expect("часть"),
            None,
            "кусок {cut} ещё не кадр"
        );
    }
    let value = decoded(raw).expect("кадр").expect("целиком");
    assert_eq!(
        value,
        Value::Array(Some(vec![
            Value::Bulk(Some(bytes::Bytes::from_static(b"foo"))),
            Value::Integer(42),
            Value::Array(Some(vec![
                Value::Simple(String::from("OK")),
                Value::Bulk(None)
            ])),
        ]))
    );
    let mut codec = RespCodec {
        max_bulk: 4,
        ..RespCodec::default()
    };
    let mut big = BytesMut::from(&b"$10\r\n0123456789\r\n"[..]);
    assert!(matches!(
        codec.decode(&mut big),
        Err(RespError::TooLarge { .. })
    ));
    let mut deep = BytesMut::from(&b"*1\r\n*1\r\n*1\r\n:1\r\n"[..]);
    let mut shallow = RespCodec {
        max_depth: 2,
        ..RespCodec::default()
    };
    assert!(matches!(
        shallow.decode(&mut deep),
        Err(RespError::TooDeep(2))
    ));
    assert!(matches!(decoded(b"?1\r\n"), Err(RespError::BadFrame(..))));
    let mut out = BytesMut::new();
    RespCodec::default()
        .encode(&cmd("SET").arg("k").arg("v"), &mut out)
        .expect("команда");
    assert_eq!(&out[..], b"*3\r\n$3\r\nSET\r\n$1\r\nk\r\n$1\r\nv\r\n");
}

#[tokio::test]
async fn pipeline_typed_commands_timeouts_and_reconnect() {
    let client = Client::connect(RedisConfig::from_env());
    client.ping().await.expect("Redis курса на 127.0.0.1:56379");
    let id = std::process::id();
    let counter = key("test", "counter", id);
    let replies = client
        .pipeline(vec![
            cmd("DEL").arg(&counter),
            cmd("INCR").arg(&counter),
            cmd("INCR").arg(&counter),
            cmd("GET").arg(&counter),
        ])
        .await
        .expect("конвейер");
    assert_eq!(
        replies[3].as_text().as_deref(),
        Some("2"),
        "ответы по порядку"
    );
    let wrong = client.query(cmd("LPUSH").arg(&counter).arg("x")).await;
    assert!(matches!(wrong, Err(RespError::Server(ref why)) if why.starts_with("WRONGTYPE")));
    let waiting = Client::connect(RedisConfig::from_env().timeout(Duration::from_millis(100)));
    let missing = key("test", "queue", id);
    assert!(matches!(
        waiting.blpop(&missing, 1).await,
        Err(RespError::Timeout)
    ));
    tokio::time::sleep(Duration::from_millis(1_200)).await;
    waiting
        .ping()
        .await
        .expect("опоздавший ответ BLPOP выброшен, а не достался PING");
    let blocking = client.dedicated();
    let pushed = async {
        tokio::time::sleep(Duration::from_millis(200)).await;
        client.query(cmd("RPUSH").arg(&missing).arg("job")).await
    };
    let blocking_cfg = Client::connect(RedisConfig::from_env().timeout(Duration::from_secs(3)));
    let (popped, _) = tokio::join!(blocking_cfg.blpop(&missing, 2), pushed);
    assert_eq!(
        popped.expect("BLPOP").as_deref(),
        Some("job"),
        "блокирующая команда на своём соединении"
    );
    let _ = blocking;
    let own: i64 = client
        .query(cmd("CLIENT").arg("ID"))
        .await
        .expect("id")
        .as_int()
        .expect("число");
    let killer = client.dedicated();
    killer
        .query(cmd("CLIENT").arg("KILL").arg("ID").arg(own))
        .await
        .expect("убить своё соединение");
    let mut back = false;
    for _ in 0..3 {
        if client.ping().await.is_ok() {
            back = true;
            break;
        }
    }
    assert!(back, "клиент переподключился сам");
    for n in 0..30 {
        client
            .set_ex(&key("test", &format!("scan{id}"), n), "1", 60)
            .await
            .expect("ключ");
    }
    let found = scan_keys(&client, &format!("social:test:scan{id}:*"), 7)
        .await
        .expect("SCAN");
    assert_eq!(found.len(), 30, "SCAN прошёл все страницы");
    client.del(&found).await.expect("уборка");
    client.del(&[counter, missing]).await.expect("уборка");
}
