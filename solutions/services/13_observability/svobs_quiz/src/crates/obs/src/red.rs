use crate::metrics::Registry;
use std::future::Future;
use std::sync::Arc;
use tokio::time::Instant;

pub const LATENCY_BOUNDS: [f64; 11] = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0];

#[derive(Clone)]
pub struct Red {
    registry: Arc<Registry>,
}

struct InFlight<'a>(&'a Registry);

impl<'a> InFlight<'a> {
    fn enter(registry: &'a Registry) -> InFlight<'a> {
        registry.gauge_add("http_requests_in_flight", "Запросы в работе прямо сейчас", 1);
        InFlight(registry)
    }
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.gauge_add("http_requests_in_flight", "Запросы в работе прямо сейчас", -1);
    }
}

impl Red {
    pub fn new(registry: Arc<Registry>) -> Red {
        Red { registry }
    }

    pub async fn track<T>(&self, route: &str, work: impl Future<Output = (u16, T)>) -> T {
        let _guard = InFlight::enter(&self.registry);
        let started = Instant::now();
        let (status, value) = work.await;
        let elapsed = started.elapsed().as_secs_f64();
        self.registry.inc("http_requests_total", "Завершённые запросы по маршрутам и кодам", &[("route", route), ("status", &status.to_string())]);
        self.registry.observe("http_request_duration_seconds", "Время ответа в секундах", &LATENCY_BOUNDS, &[("route", route)], elapsed);
        value
    }
}
