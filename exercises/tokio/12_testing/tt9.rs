// Ленивая инициализация должна выполнить дорогую работу ровно один раз, сколько бы задач ни просили
// значение одновременно. У сломанной `Racy` между проверкой и записью есть щель, и при настоящем
// параллелизме инициализация иногда выполняется дважды. Сейчас `suite` пускает четыре обращения на
// однопоточном рантайме и ничего не видит. Научи его ловить гонку надёжно: каждый запуск, не
// «иногда». Проверки внизу (`mod tests`) не трогай.
// TODO: Перепиши `suite`: на `Good` он проходит, на `Racy` падает.
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::OnceCell;

trait Lazy: Default + Send + Sync + 'static {
    fn get(&self) -> impl Future<Output = u32> + Send;
    fn inits(&self) -> u32;
}

#[derive(Default)]
struct Good {
    cell: OnceCell<u32>,
    inits: AtomicU32,
}

impl Lazy for Good {
    async fn get(&self) -> u32 {
        *self
            .cell
            .get_or_init(|| async {
                self.inits.fetch_add(1, Ordering::SeqCst);
                7
            })
            .await
    }

    fn inits(&self) -> u32 {
        self.inits.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
struct Racy {
    value: Mutex<Option<u32>>,
    inits: AtomicU32,
}

impl Lazy for Racy {
    async fn get(&self) -> u32 {
        let known = *self.value.lock().unwrap();
        if let Some(value) = known {
            return value;
        }
        std::thread::yield_now();
        self.inits.fetch_add(1, Ordering::SeqCst);
        *self.value.lock().unwrap() = Some(7);
        7
    }

    fn inits(&self) -> u32 {
        self.inits.load(Ordering::SeqCst)
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

fn suite<L: Lazy>() {
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    rt.block_on(async {
        let lazy = Arc::new(L::default());
        let askers: Vec<_> = (0..4)
            .map(|_| {
                let lazy = lazy.clone();
                tokio::spawn(async move { lazy.get().await })
            })
            .collect();
        for asker in askers {
            assert_eq!(asker.await.unwrap(), 7, "значение должно быть 7");
        }
        assert_eq!(lazy.inits(), 1, "инициализация должна выполниться один раз, а выполнена {} раз(а)", lazy.inits());
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
    fn the_suite_passes_on_the_correct_cell() {
        assert_eq!(verdict(30, suite::<Good>), Verdict::Passed, "на исправной ячейке тест обязан проходить каждый раз");
    }

    #[test]
    fn the_suite_notices_the_race_every_single_time() {
        for attempt in 1..=5 {
            assert_eq!(
                verdict(30, suite::<Racy>),
                Verdict::Failed,
                "попытка {attempt} из 5: тест пропустил гонку. Стресс-тест, который ловит её не всегда, ничего не гарантирует"
            );
        }
    }

    #[test]
    fn the_correct_cell_stays_green_many_times() {
        for attempt in 1..=5 {
            assert_eq!(verdict(30, suite::<Good>), Verdict::Passed, "попытка {attempt} из 5: тест упал на исправной ячейке");
        }
    }
}
