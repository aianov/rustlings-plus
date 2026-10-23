// Заголовок запроса приходит кусками, и после каждого куска читатель ищет в накопленном буфере
// конец заголовка `\r\n\r\n`. Читатель написан, и он честно работает, но каждый раз ищет с нуля: на
// заголовке в 6000 байт, пришедшем по байту, это 18 миллионов сравнений вместо шести тысяч. Счётчик
// `work` показывает, сколько позиций проверено. Другая ловушка: конец заголовка может прийти разрезанным
// между кусками (`\r\n` в одном и `\r\n` в следующем).
// TODO: Почини `push`: запоминай, докуда уже просмотрено, но не теряй разрезанный на стыке разделитель.
pub fn find_end(buf: &[u8], from: usize, work: &mut u64) -> Option<usize> {
    let mut at = from;
    while at + 4 <= buf.len() {
        *work += 1;
        if &buf[at..at + 4] == b"\r\n\r\n" {
            return Some(at + 4);
        }
        at += 1;
    }
    None
}

#[derive(Default)]
pub struct HeadReader {
    buf: Vec<u8>,
    scanned: usize,
    found: Option<usize>,
    pub work: u64,
}

impl HeadReader {
    pub fn new() -> HeadReader {
        HeadReader::default()
    }

    pub fn buffered(&self) -> &[u8] {
        &self.buf
    }

    pub fn push(&mut self, chunk: &[u8]) -> Option<usize> {
        self.buf.extend_from_slice(chunk);
        if self.found.is_none() {
            self.found = find_end(&self.buf, 0, &mut self.work);
        }
        self.found
    }
}

fn main() {
    let mut r = HeadReader::new();
    println!("{:?}", r.push(b"A: b\r\n\r\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_end_is_found_in_one_piece() {
        let mut r = HeadReader::new();
        assert_eq!(r.push(b"GET / HTTP/1.1\r\nHost: a\r\n\r\nbody"), Some(27));
    }

    #[test]
    fn a_separator_cut_between_chunks_is_still_found() {
        let mut r = HeadReader::new();
        assert_eq!(r.push(b"Host: a\r"), None);
        assert_eq!(r.push(b"\n\r"), None);
        assert_eq!(r.push(b"\nrest"), Some(11), "разделитель пришёл тремя кусками");
        assert_eq!(r.buffered().len(), 15);
    }

    #[test]
    fn every_split_position_works() {
        let data = b"A: 1\r\nB: 2\r\n\r\nxx";
        for cut in 1..data.len() {
            let mut r = HeadReader::new();
            let first = r.push(&data[..cut]);
            let second = r.push(&data[cut..]);
            assert_eq!(first.or(second), Some(14), "разрез после {cut} байт: конец заголовка потерян");
        }
    }

    #[test]
    fn scanning_work_is_linear_in_the_input() {
        let mut head = b"X: ".to_vec();
        head.extend(std::iter::repeat_n(b'a', 6000));
        head.extend_from_slice(b"\r\n\r\n");
        let mut r = HeadReader::new();
        let mut end = None;
        for byte in &head {
            end = end.or(r.push(std::slice::from_ref(byte)));
        }
        assert_eq!(end, Some(head.len()));
        assert!(r.work <= 2 * head.len() as u64, "проверено {} позиций на {} байт: поиск каждый раз начинается заново", r.work, head.len());
    }

    #[test]
    fn after_the_end_nothing_is_scanned_again() {
        let mut r = HeadReader::new();
        assert_eq!(r.push(b"A\r\n\r\n"), Some(5));
        let work = r.work;
        assert_eq!(r.push(b"lots of body bytes"), Some(5), "конец заголовка не меняется");
        assert_eq!(r.work, work, "заголовок уже найден, тело просматривать не надо");
    }
}
