// Рантайм переключает задачи только в точках .await. Длинный цикл без .await занимает
// поток целиком. На current_thread это значит, что соседи не получат управления вообще.
// Здесь `watch` в фоне раз за разом смотрит на прогресс `crunch`, но пока `crunch` не
// отдаёт поток, `watch` не успевает ни разу.
// TODO: Сделай так, чтобы watch успевал наблюдать за прогрессом. Результат crunch не меняй.
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

async fn watch(progress: Arc<AtomicUsize>, samples: Arc<Mutex<Vec<usize>>>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::SeqCst) {
        samples.lock().unwrap().push(progress.load(Ordering::SeqCst));
        tokio::task::yield_now().await;
    }
}

async fn crunch(progress: Arc<AtomicUsize>, steps: usize) {
    for step in 1..=steps {
        progress.store(step, Ordering::SeqCst);
        std::hint::black_box(step.wrapping_mul(31));
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let progress = Arc::new(AtomicUsize::new(0));
    crunch(Arc::clone(&progress), 1000).await;
    println!("{}", progress.load(Ordering::SeqCst));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[tokio::test]
    async fn neighbour_sees_progress_while_crunching() {
        let progress = Arc::new(AtomicUsize::new(0));
        let samples = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let watcher = tokio::spawn(watch(Arc::clone(&progress), Arc::clone(&samples), Arc::clone(&stop)));
        crunch(Arc::clone(&progress), 1000).await;
        stop.store(true, Ordering::SeqCst);
        watcher.await.unwrap();
        let seen: BTreeSet<usize> = samples.lock().unwrap().iter().copied().filter(|value| *value < 1000).collect();
        assert!(
            seen.len() >= 3,
            "сосед увидел прогресс только в {} точках: crunch не отдаёт поток, пока не закончит",
            seen.len()
        );
    }

    #[tokio::test]
    async fn crunch_still_finishes() {
        let progress = Arc::new(AtomicUsize::new(0));
        crunch(Arc::clone(&progress), 250).await;
        assert_eq!(progress.load(Ordering::SeqCst), 250, "crunch обязана дойти до конца");
    }
}
