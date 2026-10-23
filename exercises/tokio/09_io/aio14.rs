// Данные приходят кусками произвольного размера, а нам нужны целые строки. Значит, где-то
// надо копить сырые байты и отрезать от начала готовое. Копировать каждый раз в новый `Vec`
// расточительно: для этого в крейте `bytes` есть `BytesMut`, который умеет отрезать голову
// без копирования.
// TODO: Допиши `LineBuf`: `feed` добавляет кусок, `next_line` отдаёт следующую целую строку
// без `\n` (или `None`, если целой строки ещё нет) и убирает её из буфера. Недочитанный
// хвост остаётся ждать следующего куска.
use bytes::{Bytes, BytesMut};

struct LineBuf {
    buf: BytesMut,
}

impl LineBuf {
    fn new() -> LineBuf {
        LineBuf { buf: BytesMut::new() }
    }

    fn feed(&mut self, chunk: &[u8]) {
        let _ = chunk;
        todo!()
    }

    fn next_line(&mut self) -> Option<Bytes> {
        todo!()
    }

    fn pending(&self) -> usize {
        self.buf.len()
    }
}

fn main() {
    let mut lines = LineBuf::new();
    lines.feed(b"ab\ncd");
    println!("{:?} {:?}", lines.next_line(), lines.next_line());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_line_and_remainder() {
        let mut lines = LineBuf::new();
        lines.feed(b"ab\ncd");
        assert_eq!(lines.next_line().as_deref(), Some(&b"ab"[..]), "первая строка готова");
        assert_eq!(lines.next_line(), None, "вторая строка ещё не закончена");
        assert_eq!(lines.pending(), 2, "в буфере должен остаться недочитанный хвост cd");
    }

    #[test]
    fn line_assembled_from_pieces() {
        let mut lines = LineBuf::new();
        for piece in [&b"he"[..], b"l", b"lo", b"\nwo", b"rld\n"] {
            lines.feed(piece);
        }
        assert_eq!(lines.next_line().as_deref(), Some(&b"hello"[..]), "строка собирается из кусков");
        assert_eq!(lines.next_line().as_deref(), Some(&b"world"[..]), "вторая строка");
        assert_eq!(lines.pending(), 0, "после последней строки буфер пуст");
    }

    #[test]
    fn empty_line_is_a_line() {
        let mut lines = LineBuf::new();
        lines.feed(b"\n\nx\n");
        assert_eq!(lines.next_line().as_deref(), Some(&b""[..]), "пустая строка это строка");
        assert_eq!(lines.next_line().as_deref(), Some(&b""[..]), "ещё одна пустая");
        assert_eq!(lines.next_line().as_deref(), Some(&b"x"[..]), "потом x");
    }

    #[test]
    fn handed_out_line_is_not_disturbed_by_later_feeds() {
        let mut lines = LineBuf::new();
        lines.feed(b"first\n");
        let first = lines.next_line().unwrap();
        lines.feed(b"second text that is long enough to force growth\n");
        assert_eq!(&first[..], b"first", "отданная строка не должна меняться, когда буфер растёт");
    }
}
