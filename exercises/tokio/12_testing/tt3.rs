// Под замороженным временем тест двигает часы и смотрит, что произошло. Но цепочка задач (одна
// пересылает другой, та третьей) не успевает дойти до конца за один шаг: стоит передвинуть часы
// до того, как цепочка доработала, и счёт идёт неверно. Нужна функция `settle`: она пропускает
// вперёд все готовые к работе задачи, и при этом время ни на миг не сдвигается.
// TODO: Напиши `settle`.
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

async fn settle() {
    todo!()
}

fn spawn_batcher(window: Duration, sink: Arc<Mutex<Vec<Vec<u32>>>>) -> mpsc::Sender<u32> {
    let (tx, mut rx) = mpsc::channel(8);
    let (mid_tx, mut mid_rx) = mpsc::channel(8);
    tokio::spawn(async move {
        while let Some(item) = rx.recv().await {
            if mid_tx.send(item).await.is_err() {
                break;
            }
        }
    });
    tokio::spawn(async move {
        while let Some(first) = mid_rx.recv().await {
            let mut batch = vec![first];
            let deadline = sleep(window);
            tokio::pin!(deadline);
            loop {
                tokio::select! {
                    _ = &mut deadline => break,
                    next = mid_rx.recv() => match next {
                        Some(item) => batch.push(item),
                        None => break,
                    },
                }
            }
            sink.lock().unwrap().push(batch);
        }
    });
    tx
}

#[tokio::main]
async fn main() {
    settle().await;
    println!("тишина");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Instant, advance};

    #[tokio::test(start_paused = true)]
    async fn a_chain_of_relays_delivers_without_moving_time() {
        let (head, mut rx) = mpsc::channel::<u32>(1);
        for _ in 0..5 {
            let (next_tx, next_rx) = mpsc::channel::<u32>(1);
            let mut from = std::mem::replace(&mut rx, next_rx);
            tokio::spawn(async move {
                while let Some(x) = from.recv().await {
                    if next_tx.send(x).await.is_err() {
                        break;
                    }
                }
            });
        }
        let started = Instant::now();
        head.send(7).await.unwrap();
        settle().await;
        assert_eq!(rx.try_recv().ok(), Some(7), "после settle число должно пройти по всей цепочке из шести пересылок");
        assert_eq!(started.elapsed(), Duration::ZERO, "settle не имеет права двигать время");
    }

    #[tokio::test(start_paused = true)]
    async fn a_task_with_many_steps_gets_to_the_end() {
        let done = Arc::new(Mutex::new(false));
        let flag = done.clone();
        tokio::spawn(async move {
            for _ in 0..30 {
                tokio::task::yield_now().await;
            }
            *flag.lock().unwrap() = true;
        });
        settle().await;
        assert!(*done.lock().unwrap(), "задача на тридцать шагов должна доработать за один settle");
    }

    #[tokio::test(start_paused = true)]
    async fn settle_does_not_wake_timers() {
        let woke = Arc::new(Mutex::new(false));
        let flag = woke.clone();
        tokio::spawn(async move {
            sleep(Duration::from_millis(10)).await;
            *flag.lock().unwrap() = true;
        });
        settle().await;
        assert!(!*woke.lock().unwrap(), "таймер на 10 мс не должен сработать: время при settle стоит");
        advance(Duration::from_millis(10)).await;
        settle().await;
        assert!(*woke.lock().unwrap(), "после сдвига на 10 мс и settle таймер сработал");
    }

    #[tokio::test(start_paused = true)]
    async fn the_batcher_flushes_exactly_at_the_window() {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let tx = spawn_batcher(Duration::from_millis(100), sink.clone());
        tx.send(1).await.unwrap();
        settle().await;
        advance(Duration::from_millis(60)).await;
        settle().await;
        tx.send(2).await.unwrap();
        settle().await;
        advance(Duration::from_millis(39)).await;
        settle().await;
        assert!(sink.lock().unwrap().is_empty(), "на 99-й миллисекунде окно ещё не закрыто");
        advance(Duration::from_millis(1)).await;
        settle().await;
        assert_eq!(*sink.lock().unwrap(), vec![vec![1, 2]], "ровно в 100 мс окно закрывается одной пачкой [1, 2]");
    }
}
