use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

async fn roundtrip(stream: DuplexStream, payload: &[u8]) -> io::Result<Vec<u8>> {
    let (mut reader, mut writer) = tokio::io::split(stream);
    let mut echoed = vec![0u8; payload.len()];
    let (written, read) = tokio::join!(writer.write_all(payload), reader.read_exact(&mut echoed));
    written?;
    read?;
    Ok(echoed)
}

#[tokio::main]
async fn main() {
    let (client, mut server) = tokio::io::duplex(1024);
    tokio::spawn(async move {
        let mut buf = [0u8; 64];
        let n = server.read(&mut buf).await.unwrap();
        server.write_all(&buf[..n]).await.unwrap();
    });
    println!("{:?}", roundtrip(client, b"hi").await.unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    fn spawn_echo(mut server: DuplexStream) {
        tokio::spawn(async move {
            let mut buf = [0u8; 64];
            loop {
                let n = server.read(&mut buf).await.unwrap_or(0);
                if n == 0 || server.write_all(&buf[..n]).await.is_err() {
                    break;
                }
            }
        });
    }

    #[tokio::test(start_paused = true)]
    async fn big_payload_through_tiny_buffer() {
        let (client, server) = tokio::io::duplex(16);
        spawn_echo(server);
        let payload: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        let echoed = timeout(Duration::from_secs(5), roundtrip(client, &payload)).await;
        let echoed = echoed.expect("обмен завис: пока клиент пишет, он не читает, а сервер не может писать, пока клиент не читает");
        assert_eq!(echoed.unwrap(), payload, "эхо должно вернуть ровно то, что отправлено");
    }

    #[tokio::test(start_paused = true)]
    async fn small_payload_still_works() {
        let (client, server) = tokio::io::duplex(1024);
        spawn_echo(server);
        let echoed = timeout(Duration::from_secs(5), roundtrip(client, b"hello")).await.unwrap();
        assert_eq!(echoed.unwrap(), b"hello", "короткие данные проходят и при большом буфере");
    }
}
