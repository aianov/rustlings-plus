// Не хватает адаптера, который режет поток на куски: `.chunks_of(3)` превращает `1,2,3,4,5,6,7` в `[1,2,3]`,
// `[4,5,6]`, `[7]`. Последний неполный кусок отдаётся тоже. Адаптер состоит из двух частей. Структура
// `ChunksOf<I>` хранит исходный итератор и размер и сама реализует `Iterator`. Трейт-расширение
// `ChunksExt` с общей реализацией для любого итератора даёт метод `.chunks_of(n)` у всех итераторов сразу.
// Адаптер ленивый: он не должен читать из источника больше, чем нужно для очередного куска, поэтому
// работает и с бесконечными источниками. Нулевой размер кусков бессмыслен: паника с сообщением
// `chunk size must be positive` при вызове `chunks_of`.
// TODO: Реализуй `Iterator` для `ChunksOf` и метод `chunks_of` вместе с общей реализацией трейта.

pub struct ChunksOf<I: Iterator> {
    inner: I,
    size: usize,
}

impl<I: Iterator> Iterator for ChunksOf<I> {
    type Item = Vec<I::Item>;

    fn next(&mut self) -> Option<Vec<I::Item>> {
        todo!()
    }
}

pub trait ChunksExt: Iterator + Sized {
    fn chunks_of(self, size: usize) -> ChunksOf<Self>;
}

fn main() {
    let chunks: Vec<Vec<u32>> = (1..=7).chunks_of(3).collect();
    println!("{chunks:?}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_chunk_may_be_short() {
        let got: Vec<Vec<u32>> = (1..=7).chunks_of(3).collect();
        assert_eq!(got, vec![vec![1, 2, 3], vec![4, 5, 6], vec![7]]);
    }

    #[test]
    fn exact_multiple_has_no_empty_tail() {
        let got: Vec<Vec<u32>> = (1..=6).chunks_of(3).collect();
        assert_eq!(got, vec![vec![1, 2, 3], vec![4, 5, 6]], "пустого куска в конце быть не должно");
        assert_eq!(std::iter::empty::<u8>().chunks_of(2).count(), 0);
    }

    #[test]
    fn works_on_any_iterator() {
        let letters: Vec<String> = "abcde".chars().chunks_of(2).map(|c| c.into_iter().collect()).collect();
        assert_eq!(letters, vec!["ab", "cd", "e"]);
        let words: Vec<Vec<&str>> = ["a", "b", "c"].into_iter().chunks_of(5).collect();
        assert_eq!(words, vec![vec!["a", "b", "c"]], "размер больше длины: один кусок");
    }

    #[test]
    fn is_lazy_and_handles_infinite_sources() {
        let first: Vec<Vec<u64>> = (1..).chunks_of(2).take(3).collect();
        assert_eq!(first, vec![vec![1, 2], vec![3, 4], vec![5, 6]]);
        let pulled = std::cell::Cell::new(0);
        let mut it = std::iter::repeat_with(|| {
            pulled.set(pulled.get() + 1);
            pulled.get()
        })
        .chunks_of(4);
        it.next();
        assert_eq!(pulled.get(), 4, "для первого куска прочитано ровно четыре элемента");
    }

    #[test]
    #[should_panic(expected = "chunk size must be positive")]
    fn zero_size_is_rejected() {
        let _ = (1..3).chunks_of(0);
    }
}
