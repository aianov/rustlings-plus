pub fn title_from_bytes(bytes: &[u8]) -> Option<&str> {
    std::str::from_utf8(bytes).ok()
}

pub fn byte_at(buf: &[u8], index: usize) -> Option<u8> {
    buf.get(index).copied()
}

pub fn parse_header(buf: &[u8]) -> Option<(u8, u16)> {
    match buf {
        [kind, high, low, ..] => Some((*kind, u16::from_be_bytes([*high, *low]))),
        _ => None,
    }
}

fn main() {
    println!("{:?}", title_from_bytes("привет".as_bytes()));
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = include_str!("uns4.rs");
    fn code_only() -> String {
        SRC.split("#[cfg(test)]")
            .next()
            .unwrap_or("")
            .lines()
            .map(|line| line.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn valid_utf8_passes_and_garbage_does_not() {
        assert_eq!(title_from_bytes("Привет".as_bytes()), Some("Привет"));
        assert_eq!(title_from_bytes(b""), Some(""));
        assert!(title_from_bytes(&[0xff, 0xfe, 0x41]).is_none(), "это не UTF-8: строку из таких байт делать нельзя");
        assert!(title_from_bytes(&[0xd0]).is_none(), "оборванная двухбайтовая буква тоже мусор");
    }

    #[test]
    fn reading_past_the_end_gives_none() {
        let buf = [1_u8, 2, 3];
        assert_eq!(byte_at(&buf, 0), Some(1));
        assert_eq!(byte_at(&buf, 2), Some(3));
        assert_eq!(byte_at(&buf, 3), None, "индекс равен длине: байта нет");
        assert_eq!(byte_at(&buf, 4), None);
        let empty: Vec<u8> = Vec::with_capacity(16);
        assert_eq!(byte_at(&empty, 0), None);
    }

    #[test]
    fn headers_are_big_endian_and_short_buffers_are_rejected() {
        assert_eq!(parse_header(&[7, 0x01, 0x02]), Some((7, 258)), "длина идёт старшим байтом вперёд");
        assert_eq!(parse_header(&[1, 0x01, 0x02, 0xaa, 0xbb]), Some((1, 258)), "лишнее после заголовка игнорируется");
        assert_eq!(parse_header(&[7, 1]), None, "двух байт мало");
        assert_eq!(parse_header(&[7]), None);
        let empty: Vec<u8> = Vec::with_capacity(16);
        assert_eq!(parse_header(&empty), None);
    }

    #[test]
    fn no_unsafe_is_left() {
        assert!(!code_only().contains("unsafe"), "в этой задаче unsafe не нужен вовсе");
    }
}
