// read вернул Ok(0): собеседник закрыл свою сторону, больше байтов не будет. Это не ошибка,
// это конец потока (EOF). Так TCP сообщает «я всё сказал».
// TODO: Напиши с нуля read_until_close: читай из сокета кусками, пока не придёт Ok(0),
// и верни всё накопленное. Готовый read_to_end не используй, цель понять цикл.

use std::io;
use tokio::net::TcpStream;

async fn read_until_close(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    todo!()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:7").await?;
    println!("{} байт до закрытия", read_until_close(&mut stream).await?.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;
    use tokio::time::{sleep, timeout};

    async fn server_sending(parts: &'static [&'static [u8]]) -> TcpStream {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            for part in parts {
                s.write_all(part).await.unwrap();
                sleep(Duration::from_millis(20)).await;
            }
        });
        TcpStream::connect(addr).await.unwrap()
    }

    #[tokio::test]
    async fn collects_everything_before_close() {
        let mut stream = server_sending(&[b"hello", b" ", b"world"]).await;
        let got = timeout(Duration::from_secs(5), read_until_close(&mut stream))
            .await
            .expect("read_until_close завис: Ok(0) значит, что сервер закрыл соединение и пора выходить")
            .unwrap();
        assert_eq!(got, b"hello world", "собрано не всё, что отправил сервер");
    }

    #[tokio::test]
    async fn immediate_close_gives_empty_vec() {
        let mut stream = server_sending(&[]).await;
        let got = timeout(Duration::from_secs(5), read_until_close(&mut stream))
            .await
            .expect("закрытие без единого байта тоже EOF, цикл не должен зависнуть")
            .unwrap();
        assert!(got.is_empty(), "сервер ничего не присылал, а вернулось {got:?}");
    }

    #[tokio::test]
    async fn handles_more_than_one_buffer() {
        let mut stream = server_sending(&[&[7u8; 5000]]).await;
        let got = timeout(Duration::from_secs(5), read_until_close(&mut stream)).await.expect("завис").unwrap();
        assert_eq!(got.len(), 5000, "данных больше, чем влезает в один буфер: читай в цикле");
    }
}
