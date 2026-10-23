use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

async fn ask(mut stream: DuplexStream, question: &str) -> io::Result<String> {
    stream.write_all(question.as_bytes()).await?;
    stream.shutdown().await?;
    let mut answer = String::new();
    stream.read_to_string(&mut answer).await?;
    Ok(answer)
}

#[tokio::main]
async fn main() {
    let (client, mut server) = tokio::io::duplex(64);
    tokio::spawn(async move {
        let mut q = String::new();
        server.read_to_string(&mut q).await.unwrap();
        server.write_all(q.to_uppercase().as_bytes()).await.unwrap();
    });
    println!("{}", ask(client, "hi").await.unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test(start_paused = true)]
    async fn server_that_waits_for_eof_gets_it() {
        let (client, mut server) = tokio::io::duplex(64);
        tokio::spawn(async move {
            let mut q = String::new();
            server.read_to_string(&mut q).await.unwrap();
            server.write_all(q.to_uppercase().as_bytes()).await.unwrap();
            server.shutdown().await.unwrap();
        });
        let answer = timeout(Duration::from_secs(5), ask(client, "hello")).await;
        let answer = answer.expect("обмен завис: сервер ждёт конца запроса, а клиент ждёт ответа");
        assert_eq!(answer.unwrap(), "HELLO", "сервер должен получить запрос целиком и ответить");
    }

    #[tokio::test(start_paused = true)]
    async fn empty_question() {
        let (client, mut server) = tokio::io::duplex(64);
        tokio::spawn(async move {
            let mut q = String::new();
            server.read_to_string(&mut q).await.unwrap();
            server.write_all(format!("got {} bytes", q.len()).as_bytes()).await.unwrap();
        });
        let answer = timeout(Duration::from_secs(5), ask(client, "")).await;
        assert_eq!(answer.expect("завис на пустом запросе").unwrap(), "got 0 bytes", "пустой запрос тоже должен закрываться");
    }
}
