// Эхо с пульсом: всё, что прислал клиент, возвращается обратно, а раз в every секунд клиенту
// уходит байт '.', чтобы он видел, что сервер жив. Сокет здесь одолжен (&mut TcpStream), так что
// отдать его в отдельную задачу нельзя: всё должно происходить в этой одной функции.
// TODO: Допиши heartbeat_echo. Верни, сколько точек отправлено, когда клиент закрыл запись.

use std::io;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

async fn heartbeat_echo(stream: &mut TcpStream, every: Duration) -> io::Result<u32> {
    todo!()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:7").await?;
    let beats = heartbeat_echo(&mut stream, Duration::from_secs(1)).await?;
    println!("отправлено {beats} точек");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio::time::timeout;

    #[tokio::test]
    async fn echoes_and_beats() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            heartbeat_echo(&mut stream, Duration::from_millis(40)).await
        });
        let mut client = TcpStream::connect(addr).await.unwrap();
        client.write_all(b"x").await.unwrap();
        let (mut xs, mut dots) = (0, 0);
        let mut buf = [0u8; 64];
        timeout(Duration::from_secs(5), async {
            while xs < 1 || dots < 2 {
                let n = client.read(&mut buf).await.unwrap();
                assert!(n > 0, "сервер закрыл соединение, хотя клиент ещё не закрывал запись");
                for b in &buf[..n] {
                    match b {
                        b'x' => xs += 1,
                        b'.' => dots += 1,
                        other => panic!("в потоке неожиданный байт {other}"),
                    }
                }
            }
        })
        .await
        .expect("клиент не дождался эха и двух точек: либо чтение блокирует пульс, либо пульс блокирует эхо");
        assert_eq!(xs, 1, "эхо должно вернуть x ровно один раз");
        client.shutdown().await.unwrap();
        let beats = timeout(Duration::from_secs(3), server).await.expect("сервер не заметил EOF").unwrap().unwrap();
        assert!(beats >= 2, "возвращённое число точек ({beats}) меньше увиденного клиентом");
    }

    #[tokio::test]
    async fn idle_client_still_gets_heartbeats() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _ = heartbeat_echo(&mut stream, Duration::from_millis(30)).await;
        });
        let mut client = TcpStream::connect(addr).await.unwrap();
        let mut buf = [0u8; 3];
        timeout(Duration::from_secs(3), client.read_exact(&mut buf))
            .await
            .expect("клиент молчит, а пульс обязан идти: ожидание чтения не должно останавливать таймер")
            .unwrap();
        assert_eq!(&buf, b"...", "без данных от клиента приходят только точки");
    }
}
