// Источник считает значения дорого, и каждое вычисление записывается в счётчик. Потребитель
// берёт из потока первые несколько значений и уходит. Сейчас, как только поток создан,
// выполняется вся тысяча вычислений, хотя нужны три. Сделай так, чтобы поток работал по
// требованию: пока никто не опрашивает, не считается ничего, потребитель взял три значения,
// значит, посчитано три.
use futures::stream::{self, Stream, StreamExt};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Default)]
struct Source {
    computed: AtomicU32,
}

impl Source {
    fn compute(&self, n: u32) -> u64 {
        self.computed.fetch_add(1, Ordering::SeqCst);
        u64::from(n) * u64::from(n)
    }
}

fn squares(source: Arc<Source>, count: u32) -> impl Stream<Item = u64> {
    let all: Vec<u64> = (0..count).map(|n| source.compute(n)).collect();
    stream::iter(all)
}

#[tokio::main]
async fn main() {
    use futures::StreamExt;
    let source = Arc::new(Source::default());
    let first: Vec<u64> = squares(source.clone(), 1000).take(3).collect().await;
    println!("{first:?}, посчитано {}", source.computed.load(Ordering::SeqCst));
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn nothing_is_computed_until_polled() {
        let source = Arc::new(Source::default());
        let squares = squares(source.clone(), 1000);
        assert_eq!(source.computed.load(Ordering::SeqCst), 0, "поток только создан: вычислений быть не должно");
        drop(squares);
        assert_eq!(source.computed.load(Ordering::SeqCst), 0, "брошенный неопрошенный поток тоже ничего не считает");
    }

    #[tokio::test]
    async fn only_taken_values_are_computed() {
        let source = Arc::new(Source::default());
        let first: Vec<u64> = squares(source.clone(), 1000).take(3).collect().await;
        assert_eq!(first, vec![0, 1, 4], "первые три квадрата");
        assert_eq!(source.computed.load(Ordering::SeqCst), 3, "взято три значения, а посчитано {}", source.computed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn full_run() {
        let source = Arc::new(Source::default());
        let sum: u64 = squares(source.clone(), 10).fold(0, |acc, n| async move { acc + n }).await;
        assert_eq!(sum, 285, "сумма квадратов от 0 до 9");
        assert_eq!(source.computed.load(Ordering::SeqCst), 10, "каждое значение считается один раз");
    }
}
