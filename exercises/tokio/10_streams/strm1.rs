// Лента отдаёт числа по одному, а нам нужно найти первое чётное и остановиться. Функция
// принимает любой `Stream`, но компилятор не находит у него метода `next`, хотя в документации
// он есть. Заодно помни: поток ленивый, и если чётное найдено, остальное тянуть незачем.
// Разберись, почему метода нет, и добейся, чтобы тесты прошли.
use futures::stream::{self, Stream};

async fn first_even<S: Stream<Item = u32> + Unpin>(mut numbers: S) -> Option<u32> {
    while let Some(n) = numbers.next().await {
        if n % 2 == 0 {
            return Some(n);
        }
    }
    None
}

#[tokio::main]
async fn main() {
    println!("{:?}", first_even(stream::iter(vec![1, 3, 4, 5])).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn finds_the_first_even() {
        let pulled = AtomicUsize::new(0);
        let numbers = stream::iter(vec![1, 3, 4, 5, 6]).inspect(|_| {
            pulled.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(first_even(numbers).await, Some(4), "первое чётное число в ленте это 4");
        assert_eq!(pulled.load(Ordering::SeqCst), 3, "после находки тянуть дальше не нужно: поток ленивый");
    }

    #[tokio::test]
    async fn no_even_numbers() {
        assert_eq!(first_even(stream::iter(vec![1, 3, 5])).await, None, "чётных нет: ответ None");
    }

    #[tokio::test]
    async fn empty_stream() {
        assert_eq!(first_even(stream::iter(Vec::<u32>::new())).await, None, "пустая лента: ответ None");
    }
}
