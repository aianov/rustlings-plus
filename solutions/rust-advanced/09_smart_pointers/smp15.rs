use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::LazyLock;

static CODES: LazyLock<HashMap<&'static str, u32>> = LazyLock::new(|| HashMap::from([("ok", 200), ("missing", 404), ("teapot", 418)]));

static CALLS: AtomicU32 = AtomicU32::new(0);

pub fn code_of(name: &str) -> Option<u32> {
    CALLS.fetch_add(1, Ordering::SeqCst);
    CODES.get(name).copied()
}

pub fn calls() -> u32 {
    CALLS.load(Ordering::SeqCst)
}

fn main() {
    println!("{:?} {}", code_of("ok"), calls());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::thread;

    const SRC: &str = include_str!("smp15.rs");

    static SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn lookups_work() {
        let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(code_of("ok"), Some(200));
        assert_eq!(code_of("teapot"), Some(418));
        assert_eq!(code_of("nope"), None);
    }

    #[test]
    fn counting_survives_many_threads() {
        let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let before = calls();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                thread::spawn(|| {
                    for _ in 0..2000 {
                        code_of("ok");
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().expect("поток");
        }
        assert_eq!(calls() - before, 16_000, "ни один вызов не должен потеряться: счётчик общий для потоков");
    }

    #[test]
    fn no_unsafe_left() {
        let code = SRC.split("#[cfg(test)]").next().unwrap_or("");
        let code: String = code.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        assert!(!code.contains("static mut"), "static mut убери: нужна безопасная замена");
        assert!(!code.contains("unsafe"), "в решении не должно остаться unsafe");
    }
}
