use futures::stream::{Stream, StreamExt};
use std::future::ready;

async fn report<S: Stream<Item = i32> + Unpin>(readings: S) -> (Vec<(usize, i32)>, i32) {
    let good: Vec<(usize, i32)> = readings
        .filter(|n| ready(*n > 0))
        .take(3)
        .enumerate()
        .collect()
        .await;
    let sum = good.iter().map(|(_, n)| n).sum();
    (good, sum)
}

#[tokio::main]
async fn main() {
    println!("{:?}", report(futures::stream::iter(vec![5, -1, 7])).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn first_three_good_readings() {
        let pulled = AtomicUsize::new(0);
        let readings = stream::iter(vec![3, -1, 4, -5, 0, 7, 9, 10]).inspect(|_| {
            pulled.fetch_add(1, Ordering::SeqCst);
        });
        let (good, sum) = report(readings).await;
        assert_eq!(good, vec![(0, 3), (1, 4), (2, 7)], "нужны первые три положительных с номерами среди положительных");
        assert_eq!(sum, 14, "сумма трёх показаний");
        assert_eq!(pulled.load(Ordering::SeqCst), 6, "после третьего хорошего показания (на шестом элементе) читать дальше нельзя");
    }

    #[tokio::test]
    async fn fewer_than_three() {
        let (good, sum) = report(stream::iter(vec![-1, 2, 0, 5])).await;
        assert_eq!(good, vec![(0, 2), (1, 5)], "хороших всего два, отчёт короче");
        assert_eq!(sum, 7, "сумма двух");
    }

    #[tokio::test]
    async fn nothing_good() {
        let (good, sum) = report(stream::iter(vec![0, -3])).await;
        assert!(good.is_empty() && sum == 0, "хороших нет: пустой список и нулевая сумма");
    }
}
