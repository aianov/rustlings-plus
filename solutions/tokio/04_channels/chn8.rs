use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

struct Request {
    user: u32,
    reply: oneshot::Sender<String>,
}

#[derive(Debug, PartialEq)]
enum AskError {
    ServerGone,
    NoAnswer,
    TooSlow,
}

async fn ask(server: &mpsc::Sender<Request>, user: u32, wait: Duration) -> Result<String, AskError> {
    let (reply, answer) = oneshot::channel();
    server.send(Request { user, reply }).await.map_err(|_| AskError::ServerGone)?;
    match tokio::time::timeout(wait, answer).await {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(_)) => Err(AskError::NoAnswer),
        Err(_) => Err(AskError::TooSlow),
    }
}

async fn serve(mut requests: mpsc::Receiver<Request>, work: Duration) -> usize {
    let mut lost = 0;
    while let Some(request) = requests.recv().await {
        tokio::time::sleep(work).await;
        if request.reply.send(format!("профиль {}", request.user)).is_err() {
            lost += 1;
        }
    }
    lost
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel(4);
    tokio::spawn(serve(rx, Duration::from_millis(5)));
    println!("{:?}", ask(&tx, 7, Duration::from_millis(100)).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn answers_a_request() {
        let (tx, rx) = mpsc::channel(4);
        tokio::spawn(serve(rx, Duration::from_millis(10)));
        let answer = ask(&tx, 7, Duration::from_millis(100)).await;
        assert_eq!(answer, Ok("профиль 7".to_string()), "сервис отвечает за 10 мс, срок 100 мс");
    }

    #[tokio::test(start_paused = true)]
    async fn gives_up_on_time_and_server_survives() {
        let (tx, rx) = mpsc::channel(4);
        let server = tokio::spawn(serve(rx, Duration::from_millis(200)));
        let started = Instant::now();
        let answer = ask(&tx, 7, Duration::from_millis(50)).await;
        assert_eq!(answer, Err(AskError::TooSlow), "сервис работает 200 мс, ждать мы согласны 50");
        assert_eq!(started.elapsed(), Duration::from_millis(50), "клиент должен вернуться ровно по сроку");
        drop(tx);
        let lost = server.await.expect("serve запаниковал: ответ ушедшему клиенту не повод падать");
        assert_eq!(lost, 1, "один ответ так и не нашёл получателя");
    }

    #[tokio::test(start_paused = true)]
    async fn server_is_gone() {
        let (tx, rx) = mpsc::channel(4);
        drop(rx);
        let answer = ask(&tx, 7, Duration::from_millis(50)).await;
        assert_eq!(answer, Err(AskError::ServerGone), "некому принять запрос");
    }

    #[tokio::test(start_paused = true)]
    async fn server_drops_the_request() {
        let (tx, mut rx) = mpsc::channel::<Request>(4);
        tokio::spawn(async move {
            while let Some(request) = rx.recv().await {
                drop(request);
            }
        });
        let started = Instant::now();
        let answer = ask(&tx, 7, Duration::from_millis(500)).await;
        assert_eq!(answer, Err(AskError::NoAnswer), "сервис выбросил запрос, не ответив");
        assert_eq!(started.elapsed(), Duration::ZERO, "об этом можно узнать сразу, не дожидаясь срока");
    }
}
