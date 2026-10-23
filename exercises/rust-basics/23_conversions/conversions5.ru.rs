// AsRef и AsMut дают дешёвые преобразования ссылки в ссылку. Подробнее читай
// на https://doc.rust-lang.org/std/convert/trait.AsRef.html и
// https://doc.rust-lang.org/std/convert/trait.AsMut.html соответственно.

// Узнай число байт (не символов) в переданном аргументе
// (`.len()` возвращает число байт в строке).
// TODO: Добавь трейт `AsRef` как ограничение трейта, как положено.
fn byte_counter<T>(arg: T) -> usize {
    arg.as_ref().len()
}

// Узнай число символов (не байт) в переданном аргументе.
// TODO: Добавь трейт `AsRef` как ограничение трейта, как положено.
fn char_counter<T>(arg: T) -> usize {
    arg.as_ref().chars().count()
}

// Возводит число в квадрат с помощью `as_mut()`.
// TODO: Добавь подходящее ограничение трейта.
fn num_sq<T>(arg: &mut T) {
    // TODO: Реализуй тело функции.
}

fn main() {
    // Здесь можно поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn different_counts() {
        let s = "Café au lait";
        assert_ne!(char_counter(s), byte_counter(s));
    }

    #[test]
    fn same_counts() {
        let s = "Cafe au lait";
        assert_eq!(char_counter(s), byte_counter(s));
    }

    #[test]
    fn different_counts_using_string() {
        let s = String::from("Café au lait");
        assert_ne!(char_counter(s.clone()), byte_counter(s));
    }

    #[test]
    fn same_counts_using_string() {
        let s = String::from("Cafe au lait");
        assert_eq!(char_counter(s.clone()), byte_counter(s));
    }

    #[test]
    fn mut_box() {
        let mut num: Box<u32> = Box::new(3);
        num_sq(&mut num);
        assert_eq!(*num, 9);
    }
}
