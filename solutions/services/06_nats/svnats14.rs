use async_nats::{Client, ConnectOptions, Event};
use std::ops::RangeInclusive;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn main() {}

pub async fn connect_intake(url: &str, capacity: usize) -> Result<(Client, Arc<AtomicUsize>), String> {
    let slow = Arc::new(AtomicUsize::new(0));
    let counter = slow.clone();
    let client = ConnectOptions::new()
        .subscription_capacity(capacity)
        .event_callback(move |event| {
            let counter = counter.clone();
            async move {
                if let Event::SlowConsumer(_) = event {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            }
        })
        .connect(url)
        .await
        .map_err(|e| e.to_string())?;
    Ok((client, slow))
}

pub fn missing_ranges(seen: &[u64], total: u64) -> Vec<RangeInclusive<u64>> {
    let mut present = vec![false; total as usize];
    for &n in seen {
        if let Some(slot) = present.get_mut(n as usize) {
            *slot = true;
        }
    }
    let mut ranges = Vec::new();
    let mut start = None;
    for (i, here) in present.iter().enumerate() {
        match (here, start) {
            (false, None) => start = Some(i as u64),
            (true, Some(s)) => {
                ranges.push(s..=i as u64 - 1);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        ranges.push(s..=total - 1);
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::nats::{Scope, connect, listen, sync, url};
    use futures::StreamExt;
    use std::time::Duration;

    #[test]
    fn nothing_missing_when_everything_came() {
        assert!(missing_ranges(&[0, 1, 2, 3], 4).is_empty(), "всё на месте");
        assert!(missing_ranges(&[3, 1, 0, 2], 4).is_empty(), "порядок прихода не важен");
        assert!(missing_ranges(&[], 0).is_empty(), "ничего не ждали");
        assert!(missing_ranges(&[0, 1, 1, 2, 2], 3).is_empty(), "дубль это не пропажа");
    }

    #[test]
    fn holes_in_the_middle_and_at_both_ends() {
        assert_eq!(missing_ranges(&[0, 1, 4, 5], 6), vec![2..=3], "дыра посередине");
        assert_eq!(missing_ranges(&[2, 3], 5), vec![0..=1, 4..=4], "потеряно начало и хвост");
        assert_eq!(missing_ranges(&[0, 2, 4], 6), vec![1..=1, 3..=3, 5..=5], "одиночные дыры и хвост");
        assert_eq!(missing_ranges(&[], 3), vec![0..=2], "не пришло ничего");
    }

    #[test]
    fn a_lost_tail_is_visible_only_with_the_total() {
        let seen: Vec<u64> = (0..16).collect();
        assert_eq!(missing_ranges(&seen, 300), vec![16..=299], "пришли первые 16 из 300: остальные потеряны, хотя дыры в полученном нет");
    }

    #[test]
    fn numbers_beyond_the_total_are_ignored() {
        assert_eq!(missing_ranges(&[0, 1, 99], 3), vec![2..=2], "лишний номер не ломает подсчёт");
    }

    async fn burst(scope: &Scope, total: u64) {
        let publisher = connect().await;
        for n in 0..total {
            publisher.publish(scope.subject("burst"), n.to_string().into()).await.unwrap();
        }
        sync(&publisher).await;
    }

    async fn read_everything(sub: &mut async_nats::Subscriber) -> Vec<u64> {
        let mut seen = Vec::new();
        while let Ok(Some(message)) = tokio::time::timeout(Duration::from_millis(300), sub.next()).await {
            seen.push(String::from_utf8_lossy(&message.payload).parse().unwrap());
        }
        seen
    }

    async fn slow_events_settle(slow: &AtomicUsize) -> usize {
        tokio::time::sleep(Duration::from_millis(200)).await;
        slow.load(Ordering::SeqCst)
    }

    #[tokio::test]
    async fn a_small_buffer_overflows_and_the_tail_is_dropped() {
        let scope = Scope::new();
        let (client, slow) = connect_intake(&url(), 16).await.expect("подключение");
        let mut sub = listen(&client, scope.subject("burst")).await;
        burst(&scope, 300).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let seen = read_everything(&mut sub).await;
        assert!(seen.len() < 300, "буфер на 16 не мог вместить 300 нечитанных сообщений, а пришло {}", seen.len());
        assert!(seen.len() >= 16, "но первые 16 должны были лечь в буфер, пришло {}", seen.len());
        assert!(slow_events_settle(&slow).await > 0, "переполнение должно было породить события SlowConsumer");
        let lost = missing_ranges(&seen, 300);
        assert!(!lost.is_empty(), "потери должны быть видны по номерам");
        assert_eq!(*lost.last().unwrap().end(), 299, "отбрасываются новые сообщения, поэтому пропал хвост");
    }

    #[tokio::test]
    async fn a_big_enough_buffer_keeps_the_whole_burst() {
        let scope = Scope::new();
        let (client, slow) = connect_intake(&url(), 1024).await.expect("подключение");
        let mut sub = listen(&client, scope.subject("burst")).await;
        burst(&scope, 300).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let seen = read_everything(&mut sub).await;
        assert_eq!(seen.len(), 300, "буфер на 1024 вмещает всплеск в 300");
        assert!(missing_ranges(&seen, 300).is_empty(), "дыр нет");
        assert_eq!(slow_events_settle(&slow).await, 0, "и SlowConsumer не было");
    }
}
