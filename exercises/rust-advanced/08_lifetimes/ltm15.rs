// Текст из конфигов приходит с экранированием: `\n` внутри строки значит перевод строки, `\t` табуляцию,
// `\\` обратную косую. Почти все значения экранирования не содержат, и копировать каждое ради редкого случая
// расточительно. Нужна функция, которая возвращает исходную строку как есть (без аллокации), когда менять
// нечего, и новую, когда есть. Для такого и придуман `Cow<str>`: «заимствованное или владеющее».
// `unescape`: `\n`, `\t`, `\\` заменяются; неизвестные escape-последовательности (`\q`) и одинокая `\` в конце
// остаются как были.
// `squash_spaces`: подряд идущие пробелы схлопываются в один; если лишних пробелов нет, строка не копируется.
// TODO: Реализуй обе функции.
use std::borrow::Cow;

pub fn unescape(input: &str) -> Cow<'_, str> {
    todo!()
}

pub fn squash_spaces(input: &str) -> Cow<'_, str> {
    todo!()
}

fn main() {
    println!("{:?} {:?}", unescape("a\\nb"), squash_spaces("a  b"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_borrowed_from(cow: Cow<str>, source: &str) -> bool {
        matches!(cow, Cow::Borrowed(b) if b.as_ptr() == source.as_ptr() && b.len() == source.len())
    }

    #[test]
    fn plain_text_is_not_copied() {
        let s = "just text";
        assert!(is_borrowed_from(unescape(s), s), "без обратной косой копировать нечего: верни Cow::Borrowed");
        assert!(is_borrowed_from(squash_spaces(s), s), "без двойных пробелов копировать нечего");
        let e = "";
        assert!(is_borrowed_from(unescape(e), e));
    }

    #[test]
    fn escapes_are_replaced() {
        assert_eq!(unescape("a\\nb\\tc"), "a\nb\tc");
        assert_eq!(unescape("path\\\\to"), "path\\to");
        assert!(matches!(unescape("a\\nb"), Cow::Owned(_)), "если что-то изменилось, результат владеющий");
    }

    #[test]
    fn unknown_and_trailing_escapes_stay() {
        assert_eq!(unescape("a\\qb"), "a\\qb");
        assert_eq!(unescape("end\\"), "end\\");
        assert_eq!(unescape("\\\\n"), "\\n", "после двойной косой буква n уже обычная");
    }

    #[test]
    fn non_ascii_survives() {
        assert_eq!(unescape("Привет\\nмир"), "Привет\nмир");
        assert_eq!(squash_spaces("привет   мир"), "привет мир");
    }

    #[test]
    fn spaces_are_squashed() {
        assert_eq!(squash_spaces("a   b  c"), "a b c");
        assert_eq!(squash_spaces("  lead and trail  "), " lead and trail ", "крайние пробелы тоже схлопываются, но не пропадают");
        assert!(matches!(squash_spaces("a  b"), Cow::Owned(_)));
        let single = "a b c";
        assert!(is_borrowed_from(squash_spaces(single), single));
    }
}
