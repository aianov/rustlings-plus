use std::time::Duration;

fn main() {
    let pause: u64 = std::env::var("SERVICE_SLEEP_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    println!("worker started");
    std::thread::sleep(Duration::from_millis(pause));
    println!("worker stopped after {pause}ms");
}
