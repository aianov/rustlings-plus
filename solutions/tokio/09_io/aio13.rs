use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};
use tokio::sync::mpsc;

fn connect(stream: DuplexStream) -> (mpsc::Sender<String>, mpsc::Receiver<String>) {
    let (out_tx, mut out_rx) = mpsc::channel::<String>(16);
    let (in_tx, in_rx) = mpsc::channel::<String>(16);
    let (read_half, mut write_half) = io::split(stream);
    tokio::spawn(async move {
        let mut lines = BufReader::new(read_half).lines();
        while let Ok(Some(text)) = lines.next_line().await {
            if in_tx.send(text).await.is_err() {
                break;
            }
        }
    });
    tokio::spawn(async move {
        while let Some(text) = out_rx.recv().await {
            if write_half.write_all(format!("{text}\n").as_bytes()).await.is_err() {
                return;
            }
        }
        let _ = write_half.shutdown().await;
    });
    (out_tx, in_rx)
}

#[tokio::main]
async fn main() {
    let (mine, mut peer) = tokio::io::duplex(64);
    let (tx, mut rx) = connect(mine);
    tx.send("hello".to_string()).await.unwrap();
    peer.write_all(b"hi\n").await.unwrap();
    println!("{:?}", rx.recv().await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;
    use tokio::time::{sleep, timeout};

    #[tokio::test(start_paused = true)]
    async fn split_line_survives_outgoing_traffic() {
        let (mine, mut peer) = tokio::io::duplex(64);
        let (tx, mut rx) = connect(mine);
        tokio::spawn(async move {
            peer.write_all(b"he").await.unwrap();
            sleep(Duration::from_millis(5)).await;
            peer.write_all(b"llo\n").await.unwrap();
            sleep(Duration::from_secs(1)).await;
        });
        sleep(Duration::from_millis(2)).await;
        tx.send("x".to_string()).await.unwrap();
        let got = timeout(Duration::from_secs(1), rx.recv()).await.expect("строка так и не пришла");
        assert_eq!(got.as_deref(), Some("hello"), "начало строки потерялось: чтение прервали отправкой");
    }

    #[tokio::test(start_paused = true)]
    async fn outgoing_lines_reach_the_peer_and_close_is_seen() {
        let (mine, mut peer) = tokio::io::duplex(64);
        let (tx, rx) = connect(mine);
        tx.send("one".to_string()).await.unwrap();
        tx.send("two".to_string()).await.unwrap();
        drop(tx);
        let mut got = String::new();
        let read = timeout(Duration::from_secs(1), peer.read_to_string(&mut got)).await;
        read.expect("собеседник не увидел конец потока после того, как мы бросили Sender").unwrap();
        assert_eq!(got, "one\ntwo\n", "исходящие строки должны уйти по порядку, каждая с \\n");
        drop(rx);
    }

    #[tokio::test(start_paused = true)]
    async fn incoming_closes_when_peer_leaves() {
        let (mine, peer) = tokio::io::duplex(64);
        let (_tx, mut rx) = connect(mine);
        drop(peer);
        let got = timeout(Duration::from_secs(1), rx.recv()).await.expect("входящий канал не закрылся");
        assert_eq!(got, None, "собеседник ушёл: входящих строк больше не будет");
    }
}
