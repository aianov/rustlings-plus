use futures::stream::{self, Stream, StreamExt};
use tokio::sync::mpsc;

fn rx_stream<T>(rx: mpsc::Receiver<T>) -> impl Stream<Item = T> {
    stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|item| (item, rx)) })
}

fn merged(sources: Vec<mpsc::Receiver<String>>) -> impl Stream<Item = String> {
    stream::select_all(sources.into_iter().map(|rx| rx_stream(rx).boxed()))
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel(1);
    tx.send("hi".to_string()).await.unwrap();
    drop(tx);
    println!("{:?}", merged(vec![rx]).collect::<Vec<_>>().await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    fn producer(events: &'static [(u64, &'static str)]) -> mpsc::Receiver<String> {
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(async move {
            let mut now = 0;
            for (at, text) in events {
                sleep(Duration::from_millis(at - now)).await;
                now = *at;
                tx.send((*text).to_string()).await.unwrap();
            }
        });
        rx
    }

    #[tokio::test(start_paused = true)]
    async fn events_come_in_arrival_order() {
        let sources = vec![producer(&[(10, "post1"), (40, "post2")]), producer(&[(20, "like1")]), producer(&[(30, "note1")])];
        let got: Vec<String> = merged(sources).collect().await;
        assert_eq!(got, vec!["post1", "like1", "note1", "post2"], "события должны идти в порядке прихода, а не по источникам");
    }

    #[tokio::test(start_paused = true)]
    async fn ends_only_when_all_sources_end() {
        let sources = vec![producer(&[(10, "early")]), producer(&[(500, "late")])];
        let got: Vec<String> = merged(sources).collect().await;
        assert_eq!(got, vec!["early", "late"], "быстрый источник закончился, но поток должен дождаться и медленного");
    }

    #[tokio::test]
    async fn no_sources_no_events() {
        assert!(merged(vec![]).collect::<Vec<_>>().await.is_empty(), "без источников поток пуст");
    }
}
