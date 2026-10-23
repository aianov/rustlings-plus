pub fn byte_and_char_len(text: &str) -> (usize, usize) {
    (text.len(), text.chars().count())
}

pub fn sizes(text: &str) -> Vec<(char, usize)> {
    text.chars().map(|c| (c, c.len_utf8())).collect()
}

pub fn char_starts(text: &str) -> Vec<usize> {
    text.char_indices().map(|(at, _)| at).collect()
}

pub fn find_char(text: &str, needle: char) -> Option<(usize, usize)> {
    text.char_indices().enumerate().find(|(_, (_, c))| *c == needle).map(|(index, (at, _))| (at, index))
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
