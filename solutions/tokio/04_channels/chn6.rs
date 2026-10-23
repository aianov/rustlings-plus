use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Default)]
struct Probe {
    peak_middle: AtomicUsize,
}

async fn parse_stage(mut raw: mpsc::Receiver<String>, out: mpsc::Sender<String>) {
    while let Some(line) = raw.recv().await {
        out.send(line.to_uppercase()).await.unwrap();
    }
}

async fn store_stage(mut parsed: mpsc::Receiver<String>, probe: Arc<Probe>) -> Vec<String> {
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
    let (mid_tx, mid_rx) = mpsc::channel(2);
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
