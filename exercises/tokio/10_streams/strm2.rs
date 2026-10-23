// Датчик присылает показания, среди которых бывают нули и отрицательные: это сбои. Нужен отчёт:
// первые три нормальных (положительных) показания с их порядковым номером среди нормальных
// (с нуля) и сумма этих трёх. Источник может быть медленным и бесконечным, поэтому тянуть из
// него больше, чем нужно, нельзя.
// TODO: Напиши `report` из комбинаторов потока, без ручных циклов.
use futures::stream::{Stream, StreamExt};
use std::future::ready;

async fn report<S: Stream<Item = i32> + Unpin>(readings: S) -> (Vec<(usize, i32)>, i32) {
    let _ = (readings.map(|n| n), ready(()));
    todo!()
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
