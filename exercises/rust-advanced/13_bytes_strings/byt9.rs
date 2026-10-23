// Разбор HTTP-подобного запроса начинается с поиска пустой строки (`\r\n\r\n`), за которой идёт тело. Тело
// может быть чем угодно: картинкой, архивом, байтами вне UTF-8, поэтому переводить буфер в `String` нельзя.
// Всё делается на срезах байт: `windows` для поиска, `split`-подобный разбор по `\r\n`, `trim_ascii` для пробелов.
// TODO: Допиши четыре функции. Возвращай срезы исходного буфера, а не копии.
pub fn header_end(buf: &[u8]) -> Option<usize> {
    todo!("индекс сразу за первым \\r\\n\\r\\n")
}

pub fn split_head_body(buf: &[u8]) -> Option<(&[u8], &[u8])> {
    todo!("(заголовки без завершающей пустой строки, тело)")
}

pub fn header_lines(head: &[u8]) -> Vec<&[u8]> {
    todo!("строки заголовков, разделитель \\r\\n")
}

pub fn parse_header(line: &[u8]) -> Option<(&[u8], &[u8])> {
    todo!("(имя, значение без пробелов по краям), делим по первому двоеточию")
}

fn main() {
    println!("{:?}", header_end(b"A: b\r\n\r\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    const REQUEST: &[u8] = b"POST /upload HTTP/1.1\r\nHost: example.org\r\nContent-Length: 6\r\n\r\n\xff\xfe\x00\r\n\r\n\xff";

    #[test]
    fn the_first_blank_line_ends_the_head() {
        let end = header_end(REQUEST).expect("пустая строка есть");
        assert_eq!(&REQUEST[end - 4..end], b"\r\n\r\n");
        assert_eq!(end, 63, "ищем первое вхождение, а не то, что внутри тела");
        assert_eq!(header_end(b"GET / HTTP/1.1\r\nHost: a\r\n"), None);
        assert_eq!(header_end(b"\r\n\r\n"), Some(4));
    }

    #[test]
    fn head_and_body_are_split_without_utf8() {
        let (head, body) = split_head_body(REQUEST).expect("разрез есть");
        assert!(head.ends_with(b"Content-Length: 6"));
        assert_eq!(body, b"\xff\xfe\x00\r\n\r\n\xff", "в теле байты вне UTF-8 и ещё один \\r\\n\\r\\n: тело нельзя трогать");
        assert!(split_head_body(b"incomplete\r\n").is_none());
    }

    #[test]
    fn head_lines_are_slices_of_the_buffer() {
        let (head, _) = split_head_body(REQUEST).unwrap();
        let lines = header_lines(head);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], b"POST /upload HTTP/1.1");
        assert_eq!(lines[2], b"Content-Length: 6");
        let range = REQUEST.as_ptr_range();
        assert!(range.contains(&lines[1].as_ptr()), "строки должны быть срезами исходного буфера, а не копиями");
    }

    #[test]
    fn a_header_splits_on_the_first_colon_and_trims() {
        assert_eq!(parse_header(b"Host:   example.org  "), Some((&b"Host"[..], &b"example.org"[..])));
        assert_eq!(parse_header(b"Date: 10:20:30"), Some((&b"Date"[..], &b"10:20:30"[..])), "значение может содержать двоеточия");
        assert_eq!(parse_header(b"Empty:"), Some((&b"Empty"[..], &b""[..])));
        assert_eq!(parse_header(b"no colon"), None);
    }
}
