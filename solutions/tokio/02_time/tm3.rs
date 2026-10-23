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
    match tokio::time::timeout(Duration::from_millis(limit_ms), remote(delay_ms, ok)).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(message)) => Err(CallError::Failed(message)),
        Err(_) => Err(CallError::Timeout),
    }
}

async fn summarize(calls: &[(u64, bool)], limit_ms: u64) -> (usize, usize, usize) {
    let (mut good, mut failed, mut timed_out) = (0, 0, 0);
    for (delay, ok) in calls {
        match call(*delay, *ok, limit_ms).await {
            Ok(_) => good += 1,
            Err(CallError::Failed(_)) => failed += 1,
            Err(CallError::Timeout) => timed_out += 1,
        }
    }
    (good, failed, timed_out)
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
