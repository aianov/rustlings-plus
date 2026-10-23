use std::future::Future;
use std::time::Duration;
use tokio::time::{Instant, sleep};

fn main() {}

pub async fn eventually<T, F, Fut>(within: Duration, every: Duration, mut probe: F) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let deadline = Instant::now() + within;
    loop {
        let reason = match probe().await {
            Ok(value) => return Ok(value),
            Err(reason) => reason,
        };
        let now = Instant::now();
        if now >= deadline {
            return Err(format!("не дождались за {within:?}: {reason}"));
        }
        sleep(every.min(deadline - now)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[tokio::test(start_paused = true)]
    async fn a_condition_that_turns_true_is_noticed_soon_after() {
        let began = Instant::now();
        let result = eventually(MS(2000), MS(100), || async move { if began.elapsed() >= MS(350) { Ok("готово") } else { Err("рано".to_string()) } }).await;
        assert_eq!(result, Ok("готово"), "условие стало истинным на 350-й миллисекунде, до срока в две секунды");
        let waited = began.elapsed();
        assert!(waited >= MS(350) && waited <= MS(450), "ждали {waited:?}: надо было заметить сразу после 350 мс, а не ждать весь срок");
    }

    #[tokio::test(start_paused = true)]
    async fn an_already_true_condition_does_not_wait_at_all() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let began = Instant::now();
        let result = eventually(MS(1000), MS(100), move || {
            counter.fetch_add(1, Ordering::SeqCst);
            async { Ok::<u32, String>(7) }
        })
        .await;
        assert_eq!(result, Ok(7), "значение пробы возвращается как есть");
        assert_eq!(began.elapsed(), Duration::ZERO, "первая проверка делается сразу, без паузы");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "и больше не вызывается");
    }

    #[tokio::test(start_paused = true)]
    async fn a_condition_that_never_holds_fails_at_the_deadline_with_the_last_reason() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let began = Instant::now();
        let result: Result<(), String> = eventually(MS(1000), MS(300), move || {
            let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
            async move { Err(format!("попытка {n}")) }
        })
        .await;
        let message = result.unwrap_err();
        assert!(message.starts_with("не дождались за "), "сообщение начинается с «не дождались за»: {message}");
        assert!(message.ends_with("попытка 5"), "в сообщении последняя причина (проверки в 0, 300, 600, 900 и ровно в 1000 мс, всего пять): {message}");
        assert_eq!(began.elapsed(), MS(1000), "ждали ровно срок, не больше и не меньше");
    }

    #[tokio::test(start_paused = true)]
    async fn the_last_check_happens_exactly_at_the_deadline() {
        let began = Instant::now();
        let result = eventually(MS(1000), MS(300), || async move { if began.elapsed() >= MS(1000) { Ok(()) } else { Err("рано".to_string()) } }).await;
        assert_eq!(result, Ok(()), "условие стало истинным ровно в срок, и последняя проверка в срок его увидела");
    }

    #[tokio::test(start_paused = true)]
    async fn a_slow_probe_does_not_stretch_the_wait_forever() {
        let began = Instant::now();
        let result: Result<(), String> = eventually(MS(1000), MS(100), || async {
            sleep(MS(400)).await;
            Err("медленно и безуспешно".to_string())
        })
        .await;
        assert!(result.is_err(), "условие не наступило");
        let waited = began.elapsed();
        assert!(waited <= MS(1000 + 400), "ждали {waited:?}: после срока допустима только проба, которая уже началась, а счёт попыток вместо часов растянул ожидание");
    }
}
