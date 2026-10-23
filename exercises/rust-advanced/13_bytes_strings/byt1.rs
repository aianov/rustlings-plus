// Мобильный клиент показывает счётчик «осталось символов», а сервер режет текст по байтам. Это разные
// числа, и путать их нельзя: кириллическая буква занимает два байта, эмодзи четыре, а символ ASCII один.
// Нужны четыре маленьких помощника, на которых потом строится всё остальное в главе.
// TODO: Допиши функции. Смещения везде в байтах, номера символов считаются с нуля.
pub fn byte_and_char_len(text: &str) -> (usize, usize) {
    todo!("длина в байтах и в символах")
}

pub fn sizes(text: &str) -> Vec<(char, usize)> {
    todo!("каждый символ и сколько байт он занимает")
}

pub fn char_starts(text: &str) -> Vec<usize> {
    todo!("байтовое смещение начала каждого символа")
}

pub fn find_char(text: &str, needle: char) -> Option<(usize, usize)> {
    todo!("байтовое смещение и номер символа первого вхождения")
}

fn main() {
    println!("{:?}", byte_and_char_len("привет"));
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIXED: &str = "Привет, мир 👋!";

    #[test]
    fn bytes_and_chars_are_different_numbers() {
        assert_eq!(byte_and_char_len("hello"), (5, 5));
        assert_eq!(byte_and_char_len("привет"), (12, 6), "кириллица: два байта на букву");
        assert_eq!(byte_and_char_len("👍"), (4, 1));
        assert_eq!(byte_and_char_len(MIXED), (26, 14));
        assert_eq!(byte_and_char_len(""), (0, 0));
    }

    #[test]
    fn every_char_reports_its_size() {
        assert_eq!(sizes("aй👋"), vec![('a', 1), ('й', 2), ('👋', 4)]);
        let total: usize = sizes(MIXED).iter().map(|(_, n)| n).sum();
        assert_eq!(total, MIXED.len(), "размеры символов должны складываться в длину строки");
    }

    #[test]
    fn char_starts_are_byte_offsets() {
        assert_eq!(char_starts("aй👋b"), vec![0, 1, 3, 7]);
        assert!(char_starts("").is_empty());
    }

    #[test]
    fn finding_a_char_gives_byte_and_char_positions() {
        assert_eq!(find_char(MIXED, '👋'), Some((21, 12)));
        assert_eq!(find_char(MIXED, 'П'), Some((0, 0)));
        assert_eq!(find_char(MIXED, 'м'), Some((14, 8)), "первое вхождение, а не последнее");
        assert_eq!(find_char(MIXED, 'z'), None);
    }
}
