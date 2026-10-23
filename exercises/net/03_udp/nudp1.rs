// Клиенту нужно задать серверу один вопрос по UDP и получить один ответ. Никаких connect и accept:
// сокет просто привязывается к какому-нибудь адресу и шлёт куда хочет. Свой адрес выбери так, чтобы
// несколько таких вызовов подряд (и одновременно) не мешали друг другу и никому на машине.
// TODO: Допиши ask: верни ровно те байты, что прислал сервер, и адрес, откуда они пришли.

use std::io;
use std::net::SocketAddr;
use tokio::net::UdpSocket;

async fn ask(server: SocketAddr, msg: &[u8]) -> io::Result<(Vec<u8>, SocketAddr)> {
    todo!()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let server = UdpSocket::bind("127.0.0.1:0").await?;
    let addr = server.local_addr()?;
    tokio::spawn(async move {
        let mut buf = [0u8; 64];
        if let Ok((n, from)) = server.recv_from(&mut buf).await {
            let _ = server.send_to(&buf[..n], from).await;
        }
    });
    let (reply, from) = ask(addr, "привет".as_bytes()).await?;
    println!("от {from}: {}", String::from_utf8_lossy(&reply));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    async fn pong_server() -> SocketAddr {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = socket.local_addr().unwrap();
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            loop {
                let Ok((n, from)) = socket.recv_from(&mut buf).await else { return };
                let mut reply = b"pong:".to_vec();
                reply.extend_from_slice(&buf[..n]);
                let _ = socket.send_to(&reply, from).await;
            }
        });
        addr
    }

    #[tokio::test]
    async fn returns_exact_reply_and_its_source() {
        let server = pong_server().await;
        let (reply, from) = timeout(Duration::from_secs(3), ask(server, b"hi"))
            .await
            .expect("ask завис: ответ не пришёл за 3 секунды")
            .expect("ask вернул ошибку");
        assert_eq!(reply, b"pong:hi", "ответ должен быть ровно теми байтами, что прислал сервер, без хвоста буфера: {reply:?}");
        assert_eq!(from, server, "вторым значением нужен адрес отправителя ответа, то есть сервера");
    }

    #[tokio::test]
    async fn parallel_calls_do_not_collide() {
        let server = pong_server().await;
        let call = |m: &'static [u8]| timeout(Duration::from_secs(3), ask(server, m));
        let (a, b, c) = tokio::join!(call(b"a"), call(b"bb"), call(b"ccc"));
        let a = a.expect("первый вызов завис").expect("первый вызов вернул ошибку: не занимает ли он фиксированный порт?");
        let b = b.expect("второй вызов завис").expect("второй вызов вернул ошибку: не занимает ли он фиксированный порт?");
        let c = c.expect("третий вызов завис").expect("третий вызов вернул ошибку: не занимает ли он фиксированный порт?");
        assert_eq!(a.0, b"pong:a");
        assert_eq!(b.0, b"pong:bb");
        assert_eq!(c.0, b"pong:ccc");
    }
}
