// Парсер пакетов написан «быстро»: проверки убраны, потому что «данные же от нашего клиента». Но клиентом
// бывает кто угодно. Мусор в заголовке читается как будто всё в порядке, а короткий пакет заставляет читать
// чужую память. Тест подаёт именно такие данные.
// TODO: Перепиши функции так, чтобы мусор давал `None`, а ошибки не было. В конце в коде не должно остаться
// слова `unsafe`: стандартная библиотека умеет всё это безопасно.
pub fn title_from_bytes(bytes: &[u8]) -> Option<&str> {
    Some(unsafe { std::str::from_utf8_unchecked(bytes) })
}

pub fn byte_at(buf: &[u8], index: usize) -> Option<u8> {
    Some(unsafe { *buf.as_ptr().add(index) })
}

pub fn parse_header(buf: &[u8]) -> Option<(u8, u16)> {
    unsafe {
        let base = buf.as_ptr();
        let kind = *base;
        let length = u16::from_le_bytes([*base.add(1), *base.add(2)]);
        Some((kind, length))
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
