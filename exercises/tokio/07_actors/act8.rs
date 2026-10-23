// Лента собирает события из нескольких источников: у каждого своё имя, а события приходят когда
// придут. Читателю нужен один поток пар (источник, значение) в порядке прихода, который кончается,
// когда иссякли все источники. Источник описан списком (пауза перед событием, значение).
// TODO: Допиши `merge`: запусти все источники одновременно и отдай приёмник общего потока.
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

struct Feed {
    name: &'static str,
    items: Vec<(u64, u32)>,
}

fn merge(feeds: Vec<Feed>) -> mpsc::Receiver<(&'static str, u32)> {
    todo!()
}

#[tokio::main]
async fn main() {
    let mut rx = merge(vec![Feed { name: "новости", items: vec![(10, 1)] }]);
    while let Some((name, value)) = rx.recv().await {
        println!("{name}: {value}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Instant, timeout};

    async fn drain(mut rx: mpsc::Receiver<(&'static str, u32)>) -> Vec<(u128, &'static str, u32)> {
        let start = Instant::now();
        let mut seen = Vec::new();
        timeout(Duration::from_secs(60), async {
            while let Some((name, value)) = rx.recv().await {
                seen.push((start.elapsed().as_millis(), name, value));
            }
        })
        .await
        .expect("поток не закончился: после последнего события приёмник всё ещё ждёт");
        seen
    }

    #[tokio::test(start_paused = true)]
    async fn events_arrive_in_time_order_with_their_source() {
        let rx = merge(vec![
            Feed { name: "a", items: vec![(100, 1), (100, 2)] },
            Feed { name: "b", items: vec![(150, 10), (100, 20)] },
            Feed { name: "c", items: vec![] },
        ]);
        let seen = drain(rx).await;
        assert_eq!(
            seen,
            vec![(100, "a", 1), (150, "b", 10), (200, "a", 2), (250, "b", 20)],
            "источники должны работать одновременно, а события идти в порядке прихода, с именем источника"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn stream_ends_without_sources_or_items() {
        let seen = drain(merge(Vec::new())).await;
        assert!(seen.is_empty(), "источников нет, событий нет, поток сразу кончается");
        let seen = drain(merge(vec![Feed { name: "x", items: vec![] }])).await;
        assert!(seen.is_empty(), "источник без событий не должен держать поток открытым");
    }

    #[tokio::test(start_paused = true)]
    async fn many_slow_sources_take_as_long_as_the_slowest() {
        let feeds = (0..10).map(|_| Feed { name: "s", items: vec![(100, 1), (100, 2), (100, 3)] }).collect();
        let seen = drain(merge(feeds)).await;
        assert_eq!(seen.len(), 30, "десять источников по три события");
        assert_eq!(seen.last().map(|e| e.0), Some(300), "десять источников идут одновременно: весь поток занимает 300 мс");
    }
}
