// Лента постов проходит три стадии: источник выдаёт номера, вторая стадия считает по ним квадраты,
// третья оформляет строки для читателя. Каждая стадия своя задача, между ними каналы. Читатель
// забирает по строке за раз и не спешит, а источник быстрый. Память при этом не должна пухнуть:
// источник не вправе уходить вперёд от читателя больше чем на 12 элементов. Если читатель ушёл,
// вся цепочка должна остановиться, а не крутиться вхолостую. Закончились номера: поток тоже кончается.
// TODO: Допиши `start`.
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::mpsc;

fn square(n: u32) -> u64 {
    n as u64 * n as u64
}

fn render(n: u32, sq: u64) -> String {
    format!("#{n}:{sq}")
}

fn start(total: u32, produced: Arc<AtomicU32>) -> mpsc::Receiver<String> {
    todo!()
}

#[tokio::main]
async fn main() {
    let mut out = start(3, Arc::new(AtomicU32::new(0)));
    while let Some(line) = out.recv().await {
        println!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test(start_paused = true)]
    async fn source_cannot_run_far_ahead_of_a_slow_reader() {
        let produced = Arc::new(AtomicU32::new(0));
        let mut out = start(1000, produced.clone());
        for read in 1..=5u32 {
            out.recv().await.expect("поток не должен кончаться, пока номера не вышли");
            sleep(Duration::from_millis(100)).await;
            let ahead = produced.load(Ordering::SeqCst) as i64 - read as i64;
            assert!(
                ahead <= 12,
                "после {read} прочитанных источник уже выдал {} номеров, на {ahead} вперёд: очереди между стадиями безразмерные?",
                produced.load(Ordering::SeqCst)
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn everything_arrives_in_order_and_the_stream_ends() {
        let mut out = start(50, Arc::new(AtomicU32::new(0)));
        let mut lines = Vec::new();
        while let Some(line) = out.recv().await {
            lines.push(line);
        }
        let expected: Vec<String> = (0..50).map(|n| render(n, square(n))).collect();
        assert_eq!(lines, expected, "строки должны прийти все, по порядку, и поток должен закончиться сам");
    }

    #[tokio::test(start_paused = true)]
    async fn reader_leaving_stops_the_whole_chain() {
        let produced = Arc::new(AtomicU32::new(0));
        let mut out = start(1_000_000, produced.clone());
        for _ in 0..3 {
            out.recv().await.unwrap();
        }
        drop(out);
        sleep(Duration::from_secs(1)).await;
        let after_leaving = produced.load(Ordering::SeqCst);
        sleep(Duration::from_secs(1)).await;
        assert_eq!(produced.load(Ordering::SeqCst), after_leaving, "читатель ушёл, а источник всё выдаёт номера");
        assert!(after_leaving < 1000, "источник должен был остановиться вскоре после ухода читателя, а он выдал {after_leaving}");
    }
}
