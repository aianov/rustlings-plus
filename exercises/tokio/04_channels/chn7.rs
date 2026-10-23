// Подписчик ленты может отключиться посреди потока. Ничего из того, что мы ему не доставили,
// терять нельзя: недоставленное уйдёт в архив.
// TODO: Допиши две функции.
//  `forward` переправляет числа из `input` в `output`, пока подписчик жив. Когда подписчик ушёл,
//  вернуть всё, что не доставлено: и то сообщение, на котором отправка не удалась, и все остальные,
//  которые ещё придут из `input` до его закрытия, в исходном порядке.
//  `wait_until_closed` возвращается ровно в тот момент, когда получатель ушёл (опрашивать в цикле
//  нельзя, отреагировать надо сразу).
use tokio::sync::mpsc::{Receiver, Sender};

async fn forward(mut input: Receiver<u32>, output: Sender<u32>) -> Vec<u32> {
    todo!()
}

async fn wait_until_closed(tx: &Sender<u32>) {
    todo!()
}

#[tokio::main]
async fn main() {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    drop(rx);
    wait_until_closed(&tx).await;
    println!("получатель ушёл");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::sync::mpsc;
    use tokio::time::Instant;

    #[tokio::test]
    async fn nothing_is_lost_when_subscriber_leaves() {
        let (in_tx, in_rx) = mpsc::channel(16);
        for i in 0..6 {
            in_tx.send(i).await.unwrap();
        }
        drop(in_tx);
        let (out_tx, mut out_rx) = mpsc::channel(1);
        let forwarding = tokio::spawn(forward(in_rx, out_tx));
        let mut delivered = Vec::new();
        for _ in 0..2 {
            delivered.push(out_rx.recv().await.unwrap());
        }
        out_rx.close();
        while let Some(x) = out_rx.recv().await {
            delivered.push(x);
        }
        let undelivered = forwarding.await.unwrap();
        assert!(!undelivered.is_empty(), "подписчик ушёл, часть сообщений должна остаться недоставленной");
        let mut all = delivered.clone();
        all.extend(&undelivered);
        assert_eq!(
            all,
            (0..6).collect::<Vec<_>>(),
            "доставленные {delivered:?} и недоставленные {undelivered:?} вместе должны дать все сообщения по порядку. Не потеряно ли то, на котором отправка не удалась?"
        );
    }

    #[tokio::test]
    async fn healthy_subscriber_gets_everything() {
        let (in_tx, in_rx) = mpsc::channel(16);
        for i in 0..5 {
            in_tx.send(i).await.unwrap();
        }
        drop(in_tx);
        let (out_tx, mut out_rx) = mpsc::channel(16);
        let undelivered = forward(in_rx, out_tx).await;
        assert!(undelivered.is_empty(), "живой подписчик: недоставленных нет");
        let mut got = Vec::new();
        while let Some(x) = out_rx.recv().await {
            got.push(x);
        }
        assert_eq!(got, vec![0, 1, 2, 3, 4], "живой подписчик получает всё");
    }

    #[tokio::test(start_paused = true)]
    async fn wakes_exactly_when_receiver_leaves() {
        let (tx, rx) = mpsc::channel::<u32>(1);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(55)).await;
            drop(rx);
        });
        let started = Instant::now();
        wait_until_closed(&tx).await;
        assert_eq!(started.elapsed(), Duration::from_millis(55), "ждать надо ровно до ухода получателя, без опроса с шагом");
    }
}
