// timeout над операцией, которая сама возвращает Result, даёт вложенный Result<Result<T, E>, Elapsed>:
// снаружи «успели ли», внутри «что ответил сервис». Вызывающему такая вложенность не нужна, ему
// нужна одна ошибка, где таймаут отличим от отказа.
// TODO: Допиши `call`: успех отдаёт значение, отказ сервиса CallError::Failed с его текстом,
// опоздание CallError::Timeout. А `summarize` по списку вызовов считает, сколько вышло успешно,
// сколько отказов и сколько таймаутов (в таком порядке).
use std::time::Duration;

#[derive(Debug, PartialEq)]
enum CallError {
    Timeout,
    Failed(String),
}

async fn remote(delay_ms: u64, ok: bool) -> Result<u32, String> {
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    if ok { Ok(7) } else { Err("сервер отказал".to_string()) }
}

async fn call(delay_ms: u64, ok: bool, limit_ms: u64) -> Result<u32, CallError> {
    todo!()
}

async fn summarize(calls: &[(u64, bool)], limit_ms: u64) -> (usize, usize, usize) {
    todo!()
}

#[tokio::main]
async fn main() {
    println!("{:?}", call(10, true, 100).await);
    println!("{:?}", summarize(&[(10, true), (10, false), (500, true)], 100).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn success() {
        assert_eq!(call(10, true, 100).await, Ok(7), "быстрый успех");
    }

    #[tokio::test(start_paused = true)]
    async fn refusal_keeps_the_message() {
        assert_eq!(
            call(10, false, 100).await,
            Err(CallError::Failed("сервер отказал".to_string())),
            "отказ сервиса должен сохранить текст"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_is_not_a_refusal() {
        assert_eq!(call(500, true, 100).await, Err(CallError::Timeout), "опоздавший успех это таймаут");
        assert_eq!(call(500, false, 100).await, Err(CallError::Timeout), "опоздавший отказ тоже таймаут: ответа мы не увидели");
    }

    #[tokio::test(start_paused = true)]
    async fn summary_counts() {
        let calls = [(10, true), (10, false), (500, true), (20, true), (900, false)];
        assert_eq!(summarize(&calls, 100).await, (2, 1, 2), "успехи, отказы, таймауты");
    }
}
