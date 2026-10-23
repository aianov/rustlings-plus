use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::runtime::Runtime;

fn run_app() -> Vec<String> {
    let log = Arc::new(Mutex::new(Vec::new()));
    let runtime = Runtime::new().unwrap();
    let main_log = Arc::clone(&log);
    let background_log = Arc::clone(&log);
    runtime.block_on(async move {
        let flusher = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            background_log.lock().unwrap().push("сброс на диск".to_string());
        });
        main_log.lock().unwrap().push("main закончил".to_string());
        flusher.await.unwrap();
    });
    drop(runtime);
    log.lock().unwrap().to_vec()
}

fn stop_runtime(runtime: Runtime, within: Duration) {
    runtime.shutdown_timeout(within);
}

fn main() {
    println!("{:?}", run_app());
    let runtime = Runtime::new().unwrap();
    stop_runtime(runtime, Duration::from_millis(100));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn background_flush_is_not_lost() {
        assert_eq!(
            run_app(),
            ["main закончил", "сброс на диск"],
            "когда async-блок завершился, недоделанные задачи гибнут вместе с рантаймом: дождись хэндл"
        );
    }

    #[test]
    fn shutdown_does_not_wait_for_slow_blocking_work() {
        let runtime = Runtime::new().unwrap();
        runtime.spawn_blocking(|| std::thread::sleep(Duration::from_millis(700)));
        std::thread::sleep(Duration::from_millis(50));
        let started = Instant::now();
        stop_runtime(runtime, Duration::from_millis(50));
        assert!(
            started.elapsed() < Duration::from_millis(400),
            "остановка заняла {:?}: обычный drop рантайма ждёт блокирующие потоки до конца, а shutdown_timeout нет",
            started.elapsed()
        );
    }
}
