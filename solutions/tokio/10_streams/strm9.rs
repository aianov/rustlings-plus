use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

struct Feed {
    rx: mpsc::Receiver<String>,
    seq: u64,
}

impl Feed {
    fn new(rx: mpsc::Receiver<String>) -> Feed {
        Feed { rx, seq: 0 }
    }
}

impl Stream for Feed {
    type Item = (u64, String);

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(text)) => {
                self.seq += 1;
                Poll::Ready(Some((self.seq, text)))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[tokio::main]
async fn main() {
    use futures::StreamExt;
    let (tx, rx) = mpsc::channel(4);
    tx.send("hello".to_string()).await.unwrap();
    drop(tx);
    println!("{:?}", Feed::new(rx).collect::<Vec<_>>().await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use std::time::Duration;
    use tokio::time::{sleep, timeout};

    #[tokio::test(start_paused = true)]
    async fn events_arrive_numbered() {
        let (tx, rx) = mpsc::channel(4);
        tokio::spawn(async move {
            for text in ["a", "b", "c"] {
                sleep(Duration::from_millis(10)).await;
                tx.send(text.to_string()).await.unwrap();
            }
        });
        let got = timeout(Duration::from_secs(5), Feed::new(rx).collect::<Vec<_>>()).await;
        let got = got.expect("поток завис: после поступления события потребитель должен проснуться");
        let expected = vec![(1, "a".to_string()), (2, "b".to_string()), (3, "c".to_string())];
        assert_eq!(got, expected, "события должны прийти по порядку с номерами с единицы, а поток кончиться, когда отправитель ушёл");
    }

    #[tokio::test]
    async fn closed_empty_feed_ends_at_once() {
        let (tx, rx) = mpsc::channel::<String>(1);
        drop(tx);
        assert!(Feed::new(rx).collect::<Vec<_>>().await.is_empty(), "канал закрыт и пуст: поток пуст");
    }

    #[tokio::test]
    async fn works_with_combinators() {
        let (tx, rx) = mpsc::channel(8);
        for text in ["x", "yy", "zzz"] {
            tx.send(text.to_string()).await.unwrap();
        }
        drop(tx);
        let long: Vec<_> = Feed::new(rx).filter(|(_, t)| std::future::ready(t.len() > 1)).collect().await;
        assert_eq!(long, vec![(2, "yy".to_string()), (3, "zzz".to_string())], "номера считаются по всем событиям, а не по отфильтрованным");
    }
}
