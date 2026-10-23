// Соединение закрывается, если по нему ничего не происходит дольше `idle`. Активностью считается
// всё: и входящие кадры, и исходящие (другой канал). Любая активность начинает отсчёт заново.
// Если оба канала закрыты, возвращаемся сразу с причиной «никого нет»; иначе причина «простой».
// Вместе с причиной возвращаем число обработанных кадров.
// TODO: Допиши `serve`.
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep};

#[derive(Debug, PartialEq)]
enum Closed {
    Idle,
    PeersGone,
}

async fn serve(mut inbound: mpsc::Receiver<u32>, mut outbound: mpsc::Receiver<u32>, idle: Duration) -> (Closed, usize) {
    todo!()
}

#[tokio::main]
async fn main() {
    let (in_tx, inbound) = mpsc::channel(4);
    let (out_tx, outbound) = mpsc::channel(4);
    in_tx.send(1).await.unwrap();
    drop((in_tx, out_tx));
    println!("{:?}", serve(inbound, outbound, Duration::from_millis(300)).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    async fn script(tx: mpsc::Sender<u32>, times: Vec<u64>, close_at: u64) {
        let start = Instant::now();
        for (n, at) in times.into_iter().enumerate() {
            tokio::time::sleep_until(start + ms(at)).await;
            tx.send(n as u32).await.unwrap();
        }
        tokio::time::sleep_until(start + ms(close_at)).await;
    }

    async fn run(inbound: Vec<u64>, outbound: Vec<u64>, inbound_close: u64, outbound_close: u64) -> ((Closed, usize), Duration) {
        let (in_tx, in_rx) = mpsc::channel(16);
        let (out_tx, out_rx) = mpsc::channel(16);
        tokio::spawn(script(in_tx, inbound, inbound_close));
        tokio::spawn(script(out_tx, outbound, outbound_close));
        let start = Instant::now();
        let result = serve(in_rx, out_rx, ms(300)).await;
        (result, start.elapsed())
    }

    #[tokio::test(start_paused = true)]
    async fn any_frame_restarts_the_countdown() {
        let (result, elapsed) = run(vec![200, 600], vec![400], 100_000, 100_000).await;
        assert_eq!(result, (Closed::Idle, 3), "три кадра, потом простой");
        assert_eq!(elapsed, ms(900), "последний кадр на 600 мс, значит, простой наступает на 900 мс: сбрасывать отсчёт надо любым кадром, входящим и исходящим");
    }

    #[tokio::test(start_paused = true)]
    async fn silence_from_the_start() {
        let (result, elapsed) = run(vec![], vec![], 100_000, 100_000).await;
        assert_eq!(result, (Closed::Idle, 0), "никто ничего не прислал");
        assert_eq!(elapsed, ms(300), "простой отсчитывается с самого начала");
    }

    #[tokio::test(start_paused = true)]
    async fn both_peers_leave() {
        let (result, elapsed) = run(vec![10], vec![], 100, 100).await;
        assert_eq!(result, (Closed::PeersGone, 1), "оба канала закрыты: причина PeersGone");
        assert_eq!(elapsed, ms(100), "возвращаться надо сразу, не дожидаясь простоя");
    }

    #[tokio::test(start_paused = true)]
    async fn one_closed_channel_does_not_end_the_serving() {
        let (result, elapsed) = run(vec![], vec![200, 400], 50, 100_000).await;
        assert_eq!(result, (Closed::Idle, 2), "входящий закрыт, исходящий жив: работаем с ним");
        assert_eq!(elapsed, ms(700), "последний кадр на 400 мс плюс 300 мс простоя");
    }
}
