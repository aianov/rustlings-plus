use std::io;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

async fn read_line(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let n = stream.read(&mut byte).await?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        if byte[0] == b'\n' {
            return Ok(line);
        }
        line.push(byte[0]);
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:7").await?;
    println!("{:?}", read_line(&mut stream).await?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;
    use tokio::time::timeout;

    async fn client_of(payload: &'static [u8]) -> TcpStream {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            s.write_all(payload).await.unwrap();
        });
        TcpStream::connect(addr).await.unwrap()
    }

    #[tokio::test]
    async fn reads_a_full_line() {
        let mut stream = client_of(b"hello\nrest").await;
        let line = timeout(Duration::from_secs(2), read_line(&mut stream)).await.expect("завис").unwrap();
        assert_eq!(line, b"hello", "строка должна быть без '\\n' и без того, что идёт после");
    }

    #[tokio::test]
    async fn close_in_the_middle_is_an_error() {
        let mut stream = client_of(b"abc").await;
        let res = timeout(Duration::from_secs(2), read_line(&mut stream))
            .await
            .expect("read_line крутится на закрытом сокете: read вернул 0, а цикл этого не замечает");
        let err = res.expect_err("строка не закончилась переводом строки, это не успех");
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof, "ожидали UnexpectedEof, получили {err}");
    }

    #[tokio::test]
    async fn close_before_any_byte_is_an_error_too() {
        let mut stream = client_of(b"").await;
        let res = timeout(Duration::from_secs(2), read_line(&mut stream))
            .await
            .expect("read_line крутится на закрытом сокете: read вернул 0, а цикл этого не замечает");
        assert_eq!(res.expect_err("пустой поток не содержит строки").kind(), io::ErrorKind::UnexpectedEof);
    }
}
