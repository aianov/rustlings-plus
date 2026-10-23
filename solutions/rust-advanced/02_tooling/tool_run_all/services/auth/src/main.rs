use std::time::Duration;

fn main() {
    let pause: u64 = std::env::var("SERVICE_SLEEP_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    println!("auth started");
    std::thread::sleep(Duration::from_millis(pause));
    println!("auth stopped after {pause}ms");
}
