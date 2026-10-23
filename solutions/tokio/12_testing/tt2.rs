use std::time::Duration;

#[derive(Debug, PartialEq)]
struct TooSlow;

trait Render: Send + Sync + 'static {
    fn render(work_ms: u64, limit_ms: u64) -> impl Future<Output = Result<u64, TooSlow>>;
}

struct Good;
struct NoLimit;
struct AlwaysSlow;

async fn run_job(work_ms: u64) -> u64 {
    tokio::task::spawn_blocking(move || {
        std::thread::sleep(Duration::from_millis(work_ms));
        work_ms
    })
    .await
    .unwrap()
}

impl Render for Good {
    async fn render(work_ms: u64, limit_ms: u64) -> Result<u64, TooSlow> {
        tokio::time::timeout(Duration::from_millis(limit_ms), run_job(work_ms)).await.map_err(|_| TooSlow)
    }
}

impl Render for NoLimit {
    async fn render(work_ms: u64, _limit_ms: u64) -> Result<u64, TooSlow> {
        Ok(run_job(work_ms).await)
    }
}

impl Render for AlwaysSlow {
    async fn render(_work_ms: u64, _limit_ms: u64) -> Result<u64, TooSlow> {
        Err(TooSlow)
    }
}

#[derive(Debug, PartialEq)]
enum Verdict {
    Passed,
    Failed,
    Hung,
}

fn verdict<F: FnOnce() + Send + 'static>(secs: u64, run: F) -> Verdict {
    let (done, finished) = std::sync::mpsc::channel();
    let worker = std::thread::Builder::new()
        .spawn(move || {
            run();
            let _ = done.send(());
        })
        .unwrap();
    match finished.recv_timeout(Duration::from_secs(secs)) {
        Ok(()) => {
            worker.join().unwrap();
            Verdict::Passed
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Verdict::Failed,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Verdict::Hung,
    }
}

fn suite<R: Render>() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
    rt.block_on(async {
        assert_eq!(R::render(30, 1000).await, Ok(30), "работа на 30 мс при лимите в секунду должна успеть");
        assert_eq!(R::render(300, 50).await, Err(TooSlow), "работа на 300 мс при лимите 50 мс не должна успеть");
    });
}

fn main() {
    suite::<Good>();
    println!("Good проходит");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_suite_passes_on_the_correct_service() {
        assert_eq!(verdict(20, suite::<Good>), Verdict::Passed, "на исправном сервисе тест обязан проходить");
    }

    #[test]
    fn the_suite_notices_a_missing_limit() {
        assert_eq!(verdict(20, suite::<NoLimit>), Verdict::Failed, "версия без лимита ждёт сколько угодно: тест обязан это заметить");
    }

    #[test]
    fn the_suite_notices_a_service_that_always_gives_up() {
        assert_eq!(verdict(20, suite::<AlwaysSlow>), Verdict::Failed, "версия, которая отказывает всегда, не должна проходить");
    }
}
