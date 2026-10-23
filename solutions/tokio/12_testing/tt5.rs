use futures::FutureExt;
use futures::future::BoxFuture;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Clone, PartialEq)]
enum ApiError {
    Busy,
    Fatal,
}

enum Step {
    Ok(u32),
    Fail(ApiError),
    After(Duration, Box<Step>),
}

trait Api {
    fn send(&self, text: &str) -> BoxFuture<'_, Result<u32, ApiError>>;
}

async fn run(step: Step) -> Result<u32, ApiError> {
    let mut step = step;
    loop {
        match step {
            Step::Ok(id) => return Ok(id),
            Step::Fail(error) => return Err(error),
            Step::After(delay, next) => {
                sleep(delay).await;
                step = *next;
            }
        }
    }
}

async fn deliver<A: Api>(api: &A, text: &str) -> Result<u32, ApiError> {
    let mut pause = Duration::from_millis(100);
    for attempt in 1..=3 {
        match api.send(text).await {
            Ok(id) => return Ok(id),
            Err(ApiError::Busy) if attempt < 3 => {
                sleep(pause).await;
                pause *= 2;
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!()
}

struct Scripted {
    steps: Mutex<std::collections::VecDeque<Step>>,
    calls: AtomicUsize,
}

impl Scripted {
    fn new(steps: Vec<Step>) -> Scripted {
        Scripted { steps: Mutex::new(steps.into()), calls: AtomicUsize::new(0) }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Api for Scripted {
    fn send(&self, _text: &str) -> BoxFuture<'_, Result<u32, ApiError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let step = self.steps.lock().unwrap().pop_front().expect("скрипт кончился: сервис сделал лишний вызов");
        run(step).boxed()
    }
}

#[tokio::main]
async fn main() {
    let api = Scripted::new(vec![Step::Fail(ApiError::Busy), Step::Ok(1)]);
    println!("{:?}", deliver(&api, "привет").await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn succeeds_on_the_third_attempt_after_two_pauses() {
        let api = Scripted::new(vec![Step::Fail(ApiError::Busy), Step::Fail(ApiError::Busy), Step::Ok(7)]);
        let started = Instant::now();
        assert_eq!(deliver(&api, "x").await, Ok(7), "после двух «занято» третья попытка успешна");
        assert_eq!(api.calls(), 3, "три вызова, в том числе два неудачных");
        assert_eq!(started.elapsed(), Duration::from_millis(300), "паузы 100 и 200 мс");
    }

    #[tokio::test(start_paused = true)]
    async fn gives_up_after_three_busy_answers() {
        let api = Scripted::new(vec![Step::Fail(ApiError::Busy), Step::Fail(ApiError::Busy), Step::Fail(ApiError::Busy)]);
        assert_eq!(deliver(&api, "x").await, Err(ApiError::Busy), "после трёх «занято» сервис сдаётся");
        assert_eq!(api.calls(), 3, "ровно три попытки");
    }

    #[tokio::test(start_paused = true)]
    async fn a_fatal_error_is_not_retried() {
        let api = Scripted::new(vec![Step::Fail(ApiError::Fatal), Step::Ok(1)]);
        let started = Instant::now();
        assert_eq!(deliver(&api, "x").await, Err(ApiError::Fatal), "фатальная ошибка отдаётся сразу");
        assert_eq!(api.calls(), 1, "повторов нет");
        assert_eq!(started.elapsed(), Duration::ZERO, "и пауз нет");
    }

    #[tokio::test(start_paused = true)]
    async fn a_slow_answer_takes_its_time() {
        let api = Scripted::new(vec![Step::After(Duration::from_millis(250), Box::new(Step::Ok(5)))]);
        let started = Instant::now();
        assert_eq!(deliver(&api, "x").await, Ok(5), "медленный успех остаётся успехом");
        assert_eq!(started.elapsed(), Duration::from_millis(250), "задержка шага входит в ответ");
        assert_eq!(api.calls(), 1);
    }

    #[tokio::test(start_paused = true)]
    #[should_panic(expected = "лишний вызов")]
    async fn an_unscripted_call_is_loud() {
        let api = Scripted::new(vec![Step::Ok(1)]);
        let _ = api.send("a").await;
        let _ = api.send("b").await;
    }
}
