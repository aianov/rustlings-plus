// Фид-импортёр читает посты быстрее, чем медленная база их принимает. Если ничего не
// ограничивать, производитель вырвется далеко вперёд, а очередь съест память. База (`slow_sink`)
// уже написана: она берёт запись за 10 мс и замеряет, сколько записей ждёт позади.
// TODO: Допиши `feed`: передай все `items` в `slow_sink` по порядку и без потерь, при этом очередь
// между ними не должна быть длиннее `capacity`. Вернуться нужно, когда база закончила всё.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc::{self, Receiver};

#[derive(Default)]
struct Probe {
    peak_backlog: AtomicUsize,
    received: Mutex<Vec<u32>>,
}

async fn slow_sink(mut rx: Receiver<u32>, probe: Arc<Probe>) {
    while let Some(item) = rx.recv().await {
        probe.peak_backlog.fetch_max(rx.len(), Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(10)).await;
        probe.received.lock().unwrap().push(item);
    }
}

async fn feed(items: Vec<u32>, capacity: usize, probe: Arc<Probe>) {
    todo!()
}

#[tokio::main]
async fn main() {
    let probe = Arc::new(Probe::default());
    feed((0..10).collect(), 2, Arc::clone(&probe)).await;
    println!("{:?}", probe.received.lock().unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn run(n: u32, capacity: usize) -> Arc<Probe> {
        let probe = Arc::new(Probe::default());
        tokio::time::timeout(Duration::from_secs(5), feed((0..n).collect(), capacity, Arc::clone(&probe)))
            .await
            .expect("feed не закончился: проверь, что канал закрывается и что ты дожидаешься базу");
        probe
    }

    #[tokio::test(start_paused = true)]
    async fn nothing_is_lost_and_order_is_kept() {
        let probe = run(50, 3).await;
        let got = probe.received.lock().unwrap().clone();
        assert_eq!(got, (0..50).collect::<Vec<_>>(), "база должна получить все записи по порядку, ничего не потеряно");
    }

    #[tokio::test(start_paused = true)]
    async fn producer_cannot_run_far_ahead() {
        let probe = run(50, 3).await;
        let peak = probe.peak_backlog.load(Ordering::SeqCst);
        assert!(peak <= 3, "очередь выросла до {peak} при вместимости 3: производитель не ждёт места");
    }

    #[tokio::test(start_paused = true)]
    async fn tiny_capacity_still_works() {
        let probe = run(20, 1).await;
        assert_eq!(probe.received.lock().unwrap().len(), 20, "с очередью на одну запись тоже ничего не теряется");
        assert!(probe.peak_backlog.load(Ordering::SeqCst) <= 1, "очередь не длиннее одной записи");
    }

    #[tokio::test(start_paused = true)]
    async fn feed_waits_for_the_sink() {
        let probe = Arc::new(Probe::default());
        feed((0..5).collect(), 2, Arc::clone(&probe)).await;
        assert_eq!(probe.received.lock().unwrap().len(), 5, "когда feed вернулся, база уже должна обработать всё");
    }
}
