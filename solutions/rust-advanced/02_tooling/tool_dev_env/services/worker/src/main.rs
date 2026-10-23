use std::time::Duration;

fn main() {
    let pause = core_lib::pause_from(std::env::var("SERVICE_SLEEP_MS").ok().as_deref());
    println!("worker started");
    std::thread::sleep(Duration::from_millis(pause));
    println!("worker stopped after {pause}ms");
}
