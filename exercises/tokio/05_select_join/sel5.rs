// Запись считается подтверждённой, когда ответили `need` реплик из всех. Спрашиваем все реплики
// сразу, ответы нужны в том порядке, в каком они пришли, а как только набралось `need`, остальные
// запросы больше не нужны: они только занимают сеть.
// TODO: Допиши `quorum`: вернуть пары (номер реплики, ответ). Если реплик меньше, чем `need`,
// вернуть всё, что есть.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::sleep;

struct Cluster {
    delays_ms: Vec<u64>,
    finished: AtomicUsize,
}

impl Cluster {
    fn new(delays_ms: Vec<u64>) -> Cluster {
        Cluster { delays_ms, finished: AtomicUsize::new(0) }
    }

    fn size(&self) -> usize {
        self.delays_ms.len()
    }

    async fn ask(&self, replica: usize) -> u32 {
        sleep(Duration::from_millis(self.delays_ms[replica])).await;
        self.finished.fetch_add(1, Ordering::SeqCst);
        100 + replica as u32
    }
}

async fn quorum(cluster: &Cluster, need: usize) -> Vec<(usize, u32)> {
    todo!()
}

#[tokio::main]
async fn main() {
    let cluster = Cluster::new(vec![30, 10, 20]);
    println!("{:?}", quorum(&cluster, 2).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn answers_come_in_arrival_order() {
        let cluster = Cluster::new(vec![300, 100, 200, 50, 400]);
        let answers = quorum(&cluster, 3).await;
        assert_eq!(answers, vec![(3, 103), (1, 101), (2, 102)], "нужны три самых быстрых ответа в порядке прихода");
    }

    #[tokio::test(start_paused = true)]
    async fn stops_as_soon_as_quorum_is_there() {
        let cluster = Cluster::new(vec![300, 100, 200, 50, 400]);
        let start = Instant::now();
        quorum(&cluster, 3).await;
        assert_eq!(start.elapsed(), Duration::from_millis(200), "кворум набирается на третьем ответе (200 мс), дальше ждать не нужно");
        sleep(Duration::from_secs(2)).await;
        assert_eq!(cluster.finished.load(Ordering::SeqCst), 3, "запросы к остальным репликам должны быть брошены, а не доработать");
    }

    #[tokio::test(start_paused = true)]
    async fn edge_cases() {
        let cluster = Cluster::new(vec![30, 10, 20]);
        assert!(quorum(&cluster, 0).await.is_empty(), "кворум из нуля ответов не требует ничего ждать");
        let start = Instant::now();
        let all = quorum(&cluster, 10).await;
        assert_eq!(all, vec![(1, 101), (2, 102), (0, 100)], "реплик меньше, чем нужно: вернуть всё, что пришло");
        assert_eq!(start.elapsed(), Duration::from_millis(30), "ждём, пока ответят все, и не дольше");
    }
}
