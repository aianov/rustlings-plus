use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Default)]
pub struct Startup {
    started: AtomicBool,
    ready: AtomicBool,
    config: AtomicU64,
}

impl Startup {
    pub fn try_start(&self) -> bool {
        self.started.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok()
    }

    pub fn publish_config(&self, value: u64) {
        self.config.store(value, Ordering::Relaxed);
        self.ready.store(true, Ordering::Release);
    }

    pub fn config(&self) -> Option<u64> {
        if self.ready.load(Ordering::Acquire) { Some(self.config.load(Ordering::Relaxed)) } else { None }
    }
}

fn main() {
    let startup = Startup::default();
    println!("{}", startup.try_start());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::thread;

    const SRC: &str = include_str!("conc16.rs");
    fn code_only() -> String {
        SRC.split("#[cfg(test)]")
            .next()
            .unwrap_or("")
            .lines()
            .map(|line| line.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn window(code: &str, needle: &str) -> Vec<String> {
        code.match_indices(needle)
            .map(|(at, _)| code[at..].chars().take(60).collect::<String>())
            .collect()
    }

    #[test]
    fn exactly_one_thread_wins_the_start() {
        let wins = {
            let startup = Arc::new(Startup::default());
            let barrier = Arc::new(Barrier::new(8));
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    let startup = Arc::clone(&startup);
                    let barrier = Arc::clone(&barrier);
                    thread::spawn(move || {
                        barrier.wait();
                        startup.try_start()
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().expect("try_start упал с паникой")).filter(|won| *won).count()
        };
        assert_eq!(wins, 1, "право на запуск должно достаться ровно одному потоку");
    }

    #[test]
    fn the_config_appears_after_publishing() {
        let startup = Startup::default();
        assert_eq!(startup.config(), None);
        startup.publish_config(42);
        assert_eq!(startup.config(), Some(42));
    }

    #[test]
    fn publishing_uses_release_and_reading_uses_acquire() {
        let code = code_only();
        assert!(!code.contains("SeqCst"), "SeqCst не нужен");
        let stores = window(&code, "ready.store(");
        assert_eq!(stores.len(), 1);
        assert!(stores[0].contains("Release"), "флаг готовности нужно публиковать с Release");
        let loads = window(&code, "ready.load(");
        assert_eq!(loads.len(), 1);
        assert!(loads[0].contains("Acquire"), "флаг готовности нужно читать с Acquire");
        let cas = window(&code, "compare_exchange(");
        assert_eq!(cas.len(), 1);
        assert!(cas[0].contains("Acquire"), "победитель получает право с Acquire (или AcqRel)");
    }
}
