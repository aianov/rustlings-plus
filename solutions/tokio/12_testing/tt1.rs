use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

trait Ids: Default + Send + Sync + 'static {
    fn next(&self) -> u64;
    fn snapshot(&self) -> u64;
}

#[derive(Default)]
struct Good {
    n: AtomicU64,
}

impl Ids for Good {
    fn next(&self) -> u64 {
        self.n.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn snapshot(&self) -> u64 {
        tokio::task::block_in_place(|| self.n.load(Ordering::SeqCst))
    }
}

#[derive(Default)]
struct Racy {
    n: AtomicU64,
}

impl Ids for Racy {
    fn next(&self) -> u64 {
        let seen = self.n.load(Ordering::SeqCst);
        std::thread::yield_now();
        self.n.store(seen + 1, Ordering::SeqCst);
        seen + 1
    }

    fn snapshot(&self) -> u64 {
        tokio::task::block_in_place(|| self.n.load(Ordering::SeqCst))
    }
}

#[derive(Debug, PartialEq)]
enum Verdict {
    Passed,
    Failed,
    Hung,
}

fn verdict<F: FnOnce() + Send + 'static>(secs: u64, run: F) -> Verdict {
    let (done, finished) = std::sync::mpsc::channel();
    let worker = std::thread::Builder::new()
        .spawn(move || {
            run();
            let _ = done.send(());
        })
        .unwrap();
    match finished.recv_timeout(Duration::from_secs(secs)) {
        Ok(()) => {
            worker.join().unwrap();
            Verdict::Passed
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Verdict::Failed,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Verdict::Hung,
    }
}

fn suite<I: Ids>() {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(4).build().unwrap();
    rt.block_on(async {
        let ids = Arc::new(I::default());
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let ids = ids.clone();
                tokio::spawn(async move {
                    for _ in 0..200 {
                        ids.next();
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.await.unwrap();
        }
        let reader = ids.clone();
        let total = tokio::spawn(async move { reader.snapshot() }).await.unwrap();
        assert_eq!(total, 1600, "восемь задач по двести выдач: счётчик должен показать 1600, а показал {total}");
    });
}

fn main() {
    suite::<Good>();
    println!("Good проходит");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_suite_passes_on_the_correct_service() {
        assert_eq!(verdict(30, suite::<Good>), Verdict::Passed, "на исправном сервисе тест обязан проходить");
    }

    #[test]
    fn the_suite_notices_the_race() {
        assert_eq!(verdict(30, suite::<Racy>), Verdict::Failed, "в Racy выдачи теряются при параллельной работе: тест обязан это заметить");
    }
}
