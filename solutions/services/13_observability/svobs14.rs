
use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;
use tokio::time::Instant;

fn main() {}

#[derive(Debug, PartialEq)]
pub enum AppError {
    NotFound,
    Invalid,
    Internal,
}

impl AppError {
    pub fn status(&self) -> u16 {
        match self {
            AppError::NotFound => 404,
            AppError::Invalid => 422,
            AppError::Internal => 500,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Response {
    pub status: u16,
}

#[derive(Default)]
pub struct Red {
    requests: Mutex<BTreeMap<(String, u16), u64>>,
    errors: Mutex<BTreeMap<String, u64>>,
    durations: Mutex<Vec<(String, Duration)>>,
    in_flight: AtomicI64,
}

impl Red {
    pub fn requests(&self, route: &str, status: u16) -> u64 {
        self.requests.lock().expect("не отравлено").get(&(route.to_string(), status)).copied().unwrap_or(0)
    }

    pub fn errors(&self, route: &str) -> u64 {
        self.errors.lock().expect("не отравлено").get(route).copied().unwrap_or(0)
    }

    pub fn durations(&self, route: &str) -> Vec<Duration> {
        self.durations.lock().expect("не отравлено").iter().filter(|(r, _)| r == route).map(|(_, d)| *d).collect()
    }

    pub fn in_flight(&self) -> i64 {
        self.in_flight.load(Ordering::SeqCst)
    }

    fn finish(&self, route: &str, status: u16, elapsed: Duration) {
        *self.requests.lock().expect("не отравлено").entry((route.to_string(), status)).or_insert(0) += 1;
        if status >= 500 {
            *self.errors.lock().expect("не отравлено").entry(route.to_string()).or_insert(0) += 1;
        }
        self.durations.lock().expect("не отравлено").push((route.to_string(), elapsed));
    }
}

struct InFlight<'a>(&'a Red);

impl<'a> InFlight<'a> {
    fn enter(red: &'a Red) -> InFlight<'a> {
        red.in_flight.fetch_add(1, Ordering::SeqCst);
        InFlight(red)
    }
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

pub async fn instrumented<F, Fut>(red: &Red, route: &str, handler: F) -> Result<Response, AppError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Response, AppError>>,
{
    let _guard = InFlight::enter(red);
    let started = Instant::now();
    let result = handler().await;
    let status = match &result {
        Ok(response) => response.status,
        Err(error) => error.status(),
    };
    red.finish(route, status, started.elapsed());
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, timeout};

    const MS: fn(u64) -> Duration = Duration::from_millis;

    async fn after(ms: u64, result: Result<Response, AppError>) -> Result<Response, AppError> {
        sleep(MS(ms)).await;
        result
    }

    #[tokio::test(start_paused = true)]
    async fn a_success_is_counted_with_its_status_and_duration() {
        let red = Red::default();
        let out = instrumented(&red, "/feed", || after(20, Ok(Response { status: 200 }))).await;
        assert_eq!(out, Ok(Response { status: 200 }), "результат обработчика возвращается как есть");
        assert_eq!(red.requests("/feed", 200), 1);
        assert_eq!(red.durations("/feed"), [MS(20)], "длительность это время обработчика");
        assert_eq!(red.errors("/feed"), 0);
        assert_eq!(red.in_flight(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn a_client_error_is_a_request_with_a_duration_but_not_a_server_error() {
        let red = Red::default();
        let out = instrumented(&red, "/feed", || after(5, Err(AppError::NotFound))).await;
        assert_eq!(out, Err(AppError::NotFound), "ошибка возвращается наружу, обёртка её не глотает");
        assert_eq!(red.requests("/feed", 404), 1, "ответ с ошибкой тоже запрос: без него доля ошибок считается от неполного числа");
        assert_eq!(red.durations("/feed"), [MS(5)], "длительность ошибочного ответа тоже учитывается");
        assert_eq!(red.errors("/feed"), 0, "404 это ошибка клиента: сервер сработал верно");
        assert_eq!(red.in_flight(), 0, "после раннего выхода «в работе» тоже уменьшается");
    }

    #[tokio::test(start_paused = true)]
    async fn a_server_error_counts_toward_errors() {
        let red = Red::default();
        for _ in 0..3 {
            let _ = instrumented(&red, "/feed", || after(30, Err(AppError::Internal))).await;
        }
        let _ = instrumented(&red, "/feed", || after(30, Err(AppError::Invalid))).await;
        let _ = instrumented(&red, "/feed", || after(30, Ok(Response { status: 200 }))).await;
        assert_eq!(red.requests("/feed", 500), 3);
        assert_eq!(red.errors("/feed"), 3, "ошибки сервера: только 500, а 422 и 200 нет");
        assert_eq!(red.durations("/feed").len(), 5, "длительность записана у каждого завершённого запроса");
    }

    #[tokio::test(start_paused = true)]
    async fn routes_are_counted_separately() {
        let red = Red::default();
        let _ = instrumented(&red, "/feed", || after(1, Ok(Response { status: 200 }))).await;
        let _ = instrumented(&red, "/users/:id", || after(1, Err(AppError::Internal))).await;
        assert_eq!(red.requests("/feed", 200), 1);
        assert_eq!(red.requests("/users/:id", 500), 1);
        assert_eq!(red.errors("/feed"), 0);
        assert_eq!(red.errors("/users/:id"), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn in_flight_counts_the_requests_that_are_running_right_now() {
        let red = Red::default();
        let probe = async {
            sleep(MS(5)).await;
            red.in_flight()
        };
        let (a, b, c, seen) = tokio::join!(
            instrumented(&red, "/a", || after(10, Ok(Response { status: 200 }))),
            instrumented(&red, "/a", || after(10, Err(AppError::Internal))),
            instrumented(&red, "/a", || after(10, Ok(Response { status: 200 }))),
            probe
        );
        assert!(a.is_ok() && b.is_err() && c.is_ok());
        assert_eq!(seen, 3, "в момент, когда все три ещё работают, «в работе» равно трём");
        assert_eq!(red.in_flight(), 0, "когда все закончили, ноль");
    }

    #[tokio::test(start_paused = true)]
    async fn an_abandoned_request_does_not_leave_in_flight_stuck_and_is_not_counted() {
        let red = Red::default();
        let gave_up = timeout(MS(10), instrumented(&red, "/feed", || after(1000, Ok(Response { status: 200 })))).await;
        assert!(gave_up.is_err(), "клиент не дождался");
        assert_eq!(red.in_flight(), 0, "бросили на полпути, а «в работе» навсегда осталось 1: уменьшение нельзя ставить после await");
        assert_eq!(red.requests("/feed", 200), 0, "незавершённый запрос ответа не получил и в ответы не попадает");
        assert!(red.durations("/feed").is_empty());
    }
}
