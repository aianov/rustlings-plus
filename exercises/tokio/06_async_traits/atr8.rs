// Запрос к внешнему сервису иногда падает временной ошибкой, и такие вызовы надо повторять с
// растущей паузой: после первой неудачи ждём `first_delay`, после второй вдвое больше и так далее,
// после последней попытки не ждём. Пока `retry` принимает готовый future, а повторять им нечего:
// будущее нельзя дождаться второй раз.
// TODO: Перепиши `retry` так, чтобы тесты собирались и проходили: не больше `attempts` попыток,
// возвращается первый успех или ошибка последней попытки.
use std::time::Duration;
use tokio::time::sleep;

async fn retry<T, E>(attempts: usize, first_delay: Duration, op: impl std::future::Future<Output = Result<T, E>>) -> Result<T, E> {
    todo!()
}

async fn flaky_call(calls: &std::cell::Cell<u32>) -> Result<u32, String> {
    calls.set(calls.get() + 1);
    if calls.get() < 2 { Err(format!("попытка {} не удалась", calls.get())) } else { Ok(calls.get()) }
}

#[tokio::main]
async fn main() {
    let calls = std::cell::Cell::new(0);
    let result = retry(3, Duration::from_millis(10), || flaky_call(&calls)).await;
    println!("{result:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use tokio::time::Instant;

    struct Service {
        calls: Cell<usize>,
        fail_first: usize,
    }

    impl Service {
        fn new(fail_first: usize) -> Service {
            Service { calls: Cell::new(0), fail_first }
        }

        async fn call(&self) -> Result<usize, String> {
            let n = self.calls.get();
            self.calls.set(n + 1);
            if n < self.fail_first { Err(format!("fail {n}")) } else { Ok(n) }
        }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[tokio::test(start_paused = true)]
    async fn succeeds_after_temporary_failures_with_growing_pauses() {
        let service = Service::new(2);
        let start = Instant::now();
        let result = retry(5, ms(10), || service.call()).await;
        assert_eq!(result, Ok(2), "третья попытка успешна, её результат и нужен");
        assert_eq!(service.calls.get(), 3, "после успеха повторять нельзя");
        assert_eq!(start.elapsed(), ms(30), "паузы 10 и 20 мс: ждём между попытками, пауза растёт вдвое");
    }

    #[tokio::test(start_paused = true)]
    async fn gives_up_with_the_last_error() {
        let service = Service::new(100);
        let start = Instant::now();
        let result = retry(4, ms(10), || service.call()).await;
        assert_eq!(result, Err("fail 3".to_string()), "вернуться должна ошибка последней, четвёртой попытки");
        assert_eq!(service.calls.get(), 4, "ровно четыре попытки");
        assert_eq!(start.elapsed(), ms(70), "паузы 10, 20 и 40 мс, после последней попытки не ждём");
    }

    #[tokio::test(start_paused = true)]
    async fn first_success_and_single_attempt_cost_nothing() {
        let service = Service::new(0);
        let start = Instant::now();
        assert_eq!(retry(3, ms(10), || service.call()).await, Ok(0), "сразу успех");
        assert_eq!(start.elapsed(), Duration::ZERO, "успех с первой попытки: никаких пауз");
        let service = Service::new(1);
        assert_eq!(retry(1, ms(10), || service.call()).await, Err("fail 0".to_string()), "одна попытка: ошибка сразу");
        assert_eq!(start.elapsed(), Duration::ZERO, "после единственной попытки не ждём");
    }
}
