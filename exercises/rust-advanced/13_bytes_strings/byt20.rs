// Парсеры протоколов ходят по буферу слева направо и каждый раз спрашивают: достаточно ли байт? Если нет,
// это не ошибка в данных, а просто «ещё не пришло», и вызывающий дочитает сокет и повторит попытку. Поэтому
// курсор по байтам отличает `Incomplete` (сколько ещё нужно хотя бы) от `BadUtf8` (данные пришли, но это не
// текст) и не двигается при неудаче: повтор начнёт с того же места.
// TODO: Допиши `Reader`. Паники исключены: короткий буфер это `Incomplete`.
#[derive(Debug, PartialEq)]
pub enum ReadError {
    Incomplete { needed: usize },
    BadUtf8,
}

pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Reader<'a> {
        Reader { buf, pos: 0 }
    }

    pub fn position(&self) -> usize {
        todo!("сколько байт уже прочитано")
    }

    pub fn remaining(&self) -> usize {
        todo!("сколько байт осталось")
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8], ReadError> {
        todo!("n байт; не хватает: Incomplete с недостающим числом")
    }

    pub fn read_u8(&mut self) -> Result<u8, ReadError> {
        todo!("один байт")
    }

    pub fn read_u16_be(&mut self) -> Result<u16, ReadError> {
        todo!("два байта, старший первым")
    }

    pub fn read_until(&mut self, delim: u8) -> Result<&'a [u8], ReadError> {
        todo!("байты до разделителя; разделитель прочитан, но в результат не входит")
    }

    pub fn read_str(&mut self, n: usize) -> Result<&'a str, ReadError> {
        todo!("n байт как текст")
    }
}

fn main() {
    let mut r = Reader::new(b"\x00\x05hello");
    println!("{:?}", r.read_u16_be());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_read_in_order() {
        let mut r = Reader::new(&[7, 1, 2, 9]);
        assert_eq!(r.read_u8(), Ok(7));
        assert_eq!(r.read_u16_be(), Ok(0x0102));
        assert_eq!(r.position(), 3);
        assert_eq!(r.remaining(), 1);
        assert_eq!(r.read_u8(), Ok(9));
        assert_eq!(r.remaining(), 0);
    }

    #[test]
    fn a_short_buffer_says_how_much_is_missing() {
        let mut r = Reader::new(&[1]);
        assert_eq!(r.read_u16_be(), Err(ReadError::Incomplete { needed: 1 }));
        assert_eq!(r.take(5), Err(ReadError::Incomplete { needed: 4 }));
        assert_eq!(Reader::new(&[]).read_u8(), Err(ReadError::Incomplete { needed: 1 }));
    }

    #[test]
    fn a_failed_read_does_not_move_the_cursor() {
        let mut r = Reader::new(&[0, 5, b'h', b'i']);
        assert_eq!(r.read_u16_be(), Ok(5));
        assert_eq!(r.take(5), Err(ReadError::Incomplete { needed: 3 }));
        assert_eq!(r.position(), 2, "неудачное чтение не должно съедать байты: после дочитывания повтор начнётся оттуда же");
        assert_eq!(r.take(2), Ok(&b"hi"[..]));
    }

    #[test]
    fn reading_until_a_delimiter() {
        let mut r = Reader::new(b"name\nrest");
        assert_eq!(r.read_until(b'\n'), Ok(&b"name"[..]));
        assert_eq!(r.position(), 5, "разделитель съеден");
        assert_eq!(r.read_until(b'\n'), Err(ReadError::Incomplete { needed: 1 }), "разделителя ещё нет");
        assert_eq!(r.position(), 5);
        assert_eq!(Reader::new(b"\n").read_until(b'\n'), Ok(&b""[..]));
    }

    #[test]
    fn text_needs_enough_bytes_and_valid_utf8() {
        let mut r = Reader::new("привет".as_bytes());
        assert_eq!(r.read_str(4), Ok("пр"));
        assert_eq!(r.read_str(100), Err(ReadError::Incomplete { needed: 92 }));
        assert_eq!(r.read_str(3), Err(ReadError::BadUtf8), "три байта без полной буквы: данные не мусор по длине, а по содержанию");
        assert_eq!(r.position(), 4, "курсор остался перед неудавшимся чтением");
        assert_eq!(r.read_str(2), Ok("и"));
    }
}
