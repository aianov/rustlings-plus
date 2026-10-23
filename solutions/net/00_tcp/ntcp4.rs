use std::io;
use tokio::net::{TcpListener, TcpStream};

async fn connect_pair() -> io::Result<(TcpStream, TcpStream)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let (client, accepted) = tokio::join!(TcpStream::connect(addr), listener.accept());
    let (server, _) = accepted?;
    Ok((client?, server))
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let (client, server) = connect_pair().await?;
    println!("клиент {} -> сервер {}", client.local_addr()?, server.local_addr()?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::timeout;

    async fn pair() -> (TcpStream, TcpStream) {
        timeout(Duration::from_secs(5), connect_pair())
            .await
            .expect("connect_pair завис: accept и connect должны идти навстречу друг другу")
            .expect("connect_pair вернул ошибку")
    }

    #[tokio::test]
    async fn addresses_mirror_each_other() {
        let (client, server) = pair().await;
        assert_eq!(
            client.peer_addr().unwrap(),
            server.local_addr().unwrap(),
            "клиент должен считать собеседником тот адрес, на котором слушает сервер"
        );
        assert_eq!(
            server.peer_addr().unwrap(),
            client.local_addr().unwrap(),
            "сервер должен видеть клиента по его локальному адресу (с эфемерным портом)"
        );
    }

    #[tokio::test]
    async fn client_port_is_ephemeral() {
        let (client, server) = pair().await;
        let listen_port = client.peer_addr().unwrap().port();
        assert_ne!(client.local_addr().unwrap().port(), listen_port, "у клиента свой временный порт, не порт сервера");
        assert_eq!(server.local_addr().unwrap().port(), listen_port, "принятое соединение живёт на порту слушателя");
    }

    #[tokio::test]
    async fn bytes_flow_between_the_halves() {
        let (mut client, mut server) = pair().await;
        client.write_all(b"hi").await.unwrap();
        let mut buf = [0u8; 2];
        timeout(Duration::from_secs(5), server.read_exact(&mut buf))
            .await
            .expect("сервер не дождался байтов клиента: возможно, вернулись две стороны одного сокета")
            .unwrap();
        assert_eq!(&buf, b"hi", "по соединению пришло не то, что отправили");
    }
}
