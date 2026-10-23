use std::time::Duration;
use tokio::time::sleep;

#[derive(Default)]
struct Session {
    log: Vec<String>,
}

impl Session {
    async fn write(&mut self, line: &str) {
        sleep(Duration::from_millis(1)).await;
        self.log.push(line.to_string());
    }
}

async fn with_session<F, R>(work: F) -> (R, Vec<String>)
where
    F: AsyncFnOnce(&mut Session) -> R,
{
    let mut session = Session::default();
    let result = work(&mut session).await;
    (result, session.log)
}

async fn repeat_in_session<F>(times: usize, mut work: F) -> Vec<String>
where
    F: AsyncFnMut(&mut Session, usize),
{
    let mut session = Session::default();
    for round in 0..times {
        work(&mut session, round).await;
    }
    session.log
}

#[tokio::main]
async fn main() {
    let (len, log) = with_session(async |session| {
        session.write("привет").await;
        6
    })
    .await;
    println!("{len} {log:?}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn handler_borrows_the_session_and_returns_a_value() {
        let (written, log) = with_session(async |session| {
            session.write("a").await;
            session.write("b").await;
            2
        })
        .await;
        assert_eq!(written, 2, "значение обработчика возвращается наружу");
        assert_eq!(log, vec!["a", "b"], "журнал сессии содержит записи обработчика");
    }

    #[tokio::test(start_paused = true)]
    async fn handler_may_capture_the_environment() {
        let prefix = String::from("user");
        let (_, log) = with_session(async |session| {
            session.write(&format!("{prefix}-1")).await;
        })
        .await;
        assert_eq!(log, vec!["user-1"], "замыкание берёт переменную из окружения по ссылке");
        assert_eq!(prefix, "user", "переменная осталась на месте");
    }

    #[tokio::test(start_paused = true)]
    async fn repeated_handler_keeps_its_own_state() {
        let mut total = 0;
        let log = repeat_in_session(3, async |session, round| {
            session.write(&format!("round {round}")).await;
            total += round;
        })
        .await;
        assert_eq!(log, vec!["round 0", "round 1", "round 2"], "в одной сессии три записи подряд");
        assert_eq!(total, 3, "обработчик меняет захваченную переменную между вызовами");
    }
}
