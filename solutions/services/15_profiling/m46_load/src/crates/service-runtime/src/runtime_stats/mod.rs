use tokio::runtime::Handle;

use crate::metrics::Metrics;

pub const ALIVE_TASKS: &str = "tokio_alive_tasks";

pub const GLOBAL_QUEUE: &str = "tokio_global_queue_depth";

pub const WORKERS: &str = "tokio_workers";

pub fn record_runtime(metrics: &Metrics) -> bool {
    let Ok(handle) = Handle::try_current() else {
        return false;
    };
    let runtime = handle.metrics();
    let gauge = |value: usize| i64::try_from(value).unwrap_or(i64::MAX);
    metrics.set(
        ALIVE_TASKS,
        "живые задачи рантайма tokio",
        &[],
        gauge(runtime.num_alive_tasks()),
    );
    metrics.set(
        GLOBAL_QUEUE,
        "задачи в глобальной очереди tokio: растёт, когда воркеры не успевают",
        &[],
        gauge(runtime.global_queue_depth()),
    );
    metrics.set(
        WORKERS,
        "воркеры рантайма tokio",
        &[],
        gauge(runtime.num_workers()),
    );
    true
}
