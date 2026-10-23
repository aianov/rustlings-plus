// Прокси между клиентом и сервером: всё, что пришло от клиента, уходит серверу, и обратно.
// Оба направления живут одновременно: сервер может ответить, пока клиент ещё пишет, а клиент
// ждёт ответа, не закрывая запись. Когда обе стороны закончили, прокси сообщает, сколько байт
// прошло в каждую сторону.
// TODO: Напиши `relay`, он возвращает пару `(от клиента серверу, от сервера клиенту)`.
use std::io;
use tokio::io::DuplexStream;

async fn relay(client_side: DuplexStream, server_side: DuplexStream) -> io::Result<(u64, u64)> {
    let _ = (client_side, server_side);
    todo!()
}

#[tokio::main]
async fn main() {
    let (client, proxy_a) = tokio::io::duplex(64);
    let (proxy_b, server) = tokio::io::duplex(64);
    drop((client, server));
    println!("{:?}", relay(proxy_a, proxy_b).await.unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt};
    use tokio::time::timeout;

    #[tokio::test(start_paused = true)]
    async fn ping_pong_through_the_proxy() {
        let (mut client, proxy_a) = duplex(64);
        let (proxy_b, mut server) = duplex(64);
        let proxy = tokio::spawn(relay(proxy_a, proxy_b));

        tokio::spawn(async move {
            let mut buf = [0u8; 32];
            loop {
                let n = server.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                server.write_all(&buf[..n].to_ascii_uppercase()).await.unwrap();
            }
            server.shutdown().await.unwrap();
        });

        let script = async {
            client.write_all(b"ping").await.unwrap();
            let mut reply = [0u8; 4];
            client.read_exact(&mut reply).await.unwrap();
            assert_eq!(&reply, b"PING", "первый ответ сервера должен дойти до клиента, пока тот не закрыл запись");
            client.write_all(b"pong two").await.unwrap();
            let mut reply = [0u8; 8];
            client.read_exact(&mut reply).await.unwrap();
            assert_eq!(&reply, b"PONG TWO", "второй обмен через тот же прокси");
            client.shutdown().await.unwrap();
            let mut rest = Vec::new();
            client.read_to_end(&mut rest).await.unwrap();
            assert!(rest.is_empty(), "после закрытия клиентом сервер ничего больше не шлёт");
        };
        timeout(Duration::from_secs(5), script).await.expect("обмен завис: прокси гонит данные только в одну сторону за раз");

        let counts = timeout(Duration::from_secs(5), proxy).await.expect("прокси не завершился").unwrap().unwrap();
        assert_eq!(counts, (12, 12), "прокси должен сообщить, сколько байт прошло в каждую сторону");
    }
}
