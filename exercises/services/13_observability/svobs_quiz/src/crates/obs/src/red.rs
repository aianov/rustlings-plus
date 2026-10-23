use crate::metrics::Registry;
use std::future::Future;
use std::sync::Arc;

pub const LATENCY_BOUNDS: [f64; 11] = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0];

#[derive(Clone)]
pub struct Red {
    registry: Arc<Registry>,
}

impl Red {
    pub fn new(registry: Arc<Registry>) -> Red {
        Red { registry }
    }

    pub async fn track<T>(&self, route: &str, work: impl Future<Output = (u16, T)>) -> T {
        let _ = (&self.registry, route, work);
        todo!()
    }
}
