// Обёртка `Watched<T>` ведёт учёт: сколько раз в значение заглядывали и сколько раз его меняли (для отладки
// кэшей и для тестов). Хочется, чтобы она вела себя как сам `T`: у `Watched<String>` работает `len()`, её
// можно передать туда, где ждут `&str`, а `push_str` меняет внутренний текст. Для этого нужны трейты `Deref`
// и `DerefMut`. Чтение через `deref(&self)` получает только общую ссылку, поэтому счётчик чтений придётся
// держать в `Cell`. Само создание значения чтением не считается.
// TODO: Реализуй `Deref` (чтение засчитывается) и `DerefMut` (запись засчитывается, чтение нет).
use std::cell::Cell;
use std::ops::{Deref, DerefMut};

pub struct Watched<T> {
    value: T,
    reads: Cell<u32>,
    writes: u32,
}

impl<T> Watched<T> {
    pub fn new(value: T) -> Watched<T> {
        Watched { value, reads: Cell::new(0), writes: 0 }
    }

    pub fn reads(&self) -> u32 {
        self.reads.get()
    }

    pub fn writes(&self) -> u32 {
        self.writes
    }

    pub fn into_inner(self) -> T {
        self.value
    }
}

fn main() {
    let w = Watched::new(String::from("hi"));
    println!("{} {}", w.len(), w.reads());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shout(text: &str) -> String {
        text.to_uppercase()
    }

    #[test]
    fn behaves_like_the_inner_value() {
        let w = Watched::new(String::from("hello"));
        assert_eq!(w.len(), 5);
        assert!(w.starts_with("he"));
        assert_eq!(w.reads(), 2, "каждый вызов метода через Watched это одно чтение");
        assert_eq!(w.writes(), 0);
    }

    #[test]
    fn deref_coercion_reaches_str() {
        let w = Watched::new(String::from("quiet"));
        assert_eq!(shout(&w), "QUIET", "&Watched<String> должна подходить туда, где ждут &str");
        assert_eq!(w.reads(), 1);
    }

    #[test]
    fn writes_are_counted_separately() {
        let mut w = Watched::new(String::from("a"));
        w.push_str("bc");
        *w = format!("{}!", *w);
        assert_eq!(w.writes(), 2, "push_str и присваивание через * это две записи");
        assert_eq!(w.reads(), 1, "в `format!` значение прочитано один раз, записи чтениями не считаются");
        assert_eq!(&*w, "abc!");
    }

    #[test]
    fn works_with_other_types() {
        let mut v = Watched::new(vec![3, 1, 2]);
        v.sort();
        let total: i32 = v.iter().sum();
        assert_eq!((total, v.writes(), v.reads()), (6, 1, 1));
        assert_eq!(v.into_inner(), vec![1, 2, 3]);
    }

    #[test]
    fn fresh_wrapper_is_untouched() {
        let w = Watched::new(5u8);
        assert_eq!((w.reads(), w.writes()), (0, 0));
    }
}
