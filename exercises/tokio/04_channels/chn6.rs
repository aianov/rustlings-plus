// Лог-конвейер из двух стадий: `parse_stage` быстро разбирает строки, `store_stage` медленно
// складывает их в хранилище (10 мс на строку). Входной канал ограничен четырьмя местами, так что
// всё должно быть под контролем. Но на нагрузке память растёт: между стадиями копится огромная
// очередь, и проба `peak_middle` это показывает.
// TODO: Найди, где ограничение теряется, и почини, не теряя строк и не меняя порядок.
// Тест вызывает только `run`.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Default)]
struct Probe {
    peak_middle: AtomicUsize,
}

async fn parse_stage(mut raw: mpsc::Receiver<String>, out: mpsc::UnboundedSender<String>) {
    while let Some(line) = raw.recv().await {
        out.send(line.to_uppercase()).unwrap();
    }
}

async fn store_stage(mut parsed: mpsc::UnboundedReceiver<String>, probe: Arc<Probe>) -> Vec<String> {
    let mut stored = Vec::new();
    while let Some(line) = parsed.recv().await {
        probe.peak_middle.fetch_max(parsed.len(), Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(10)).await;
        stored.push(line);
    }
    stored
}

async fn run(lines: Vec<String>, probe: Arc<Probe>) -> Vec<String> {
    let (raw_tx, raw_rx) = mpsc::channel(4);
    let (mid_tx, mid_rx) = mpsc::unbounded_channel();
    let parse = tokio::spawn(parse_stage(raw_rx, mid_tx));
    let store = tokio::spawn(store_stage(mid_rx, probe));
    for line in lines {
        raw_tx.send(line).await.unwrap();
    }
    drop(raw_tx);
    parse.await.unwrap();
    store.await.unwrap()
}

#[tokio::main]
async fn main() {
    let probe = Arc::new(Probe::default());
    let out = run(vec!["a".into(), "b".into()], Arc::clone(&probe)).await;
    println!("{out:?}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("строка {i}")).collect()
    }

    #[tokio::test(start_paused = true)]
    async fn nothing_lost_in_order() {
        let probe = Arc::new(Probe::default());
        let out = run(lines(100), Arc::clone(&probe)).await;
        let expected: Vec<String> = lines(100).iter().map(|l| l.to_uppercase()).collect();
        assert_eq!(out, expected, "строки должны дойти все, по порядку и разобранными (в верхнем регистре)");
    }

    #[tokio::test(start_paused = true)]
    async fn backlog_between_stages_is_bounded() {
        let probe = Arc::new(Probe::default());
        run(lines(100), Arc::clone(&probe)).await;
        let peak = probe.peak_middle.load(Ordering::SeqCst);
        assert!(peak <= 4, "между стадиями накопилось {peak} строк: ограничение первой стадии не дошло до хранилища");
    }
}
