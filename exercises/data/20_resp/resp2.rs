// GET вернул то, что лежит под ключом: имя, картинку или кусок сериализованной структуры. Для
// произвольных байтов в RESP есть отдельный вид ответа, у него в заголовке длина, а не конец строки.
// Отсутствие значения кодируется отдельно, и это не то же самое, что пустое значение.
// TODO: Закодируй значение (или его отсутствие) для ответа на GET.

fn bulk(value: Option<&[u8]>) -> Vec<u8> {
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_value() {
        assert_eq!(bulk(Some(b"hello")), b"$5\r\nhello\r\n".to_vec(), "длина, CRLF, байты, CRLF");
    }

    #[test]
    fn empty_value_is_not_missing_value() {
        assert_eq!(bulk(Some(b"")), b"$0\r\n\r\n".to_vec(), "пустая строка: длина 0 и всё равно два CRLF");
        assert_eq!(bulk(None), b"$-1\r\n".to_vec(), "нет значения: длина -1 и никакого тела");
    }

    #[test]
    fn length_counts_bytes_not_characters() {
        let text = "привет";
        assert_eq!(bulk(Some(text.as_bytes())), [b"$12\r\n".as_slice(), text.as_bytes(), b"\r\n"].concat(), "в «привет» шесть букв, но тринадцать байтов в UTF-8");
    }

    #[test]
    fn binary_value_passes_unchanged() {
        let raw = [0xff, 0x00, b'\r', b'\n', 0xfe];
        assert_eq!(bulk(Some(&raw)), [b"$5\r\n".as_slice(), &raw, b"\r\n"].concat(), "внутри значения могут быть любые байты, в том числе CRLF и нули; их не трогают и не экранируют");
    }

    #[test]
    fn big_value() {
        let big = vec![b'x'; 100_000];
        let out = bulk(Some(&big));
        assert!(out.starts_with(b"$100000\r\n"), "заголовок большого значения");
        assert_eq!(out.len(), 9 + 100_000 + 2, "длина всего кадра");
    }
}
