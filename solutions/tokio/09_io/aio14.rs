use bytes::{Bytes, BytesMut};

struct LineBuf {
    buf: BytesMut,
}

impl LineBuf {
    fn new() -> LineBuf {
        LineBuf { buf: BytesMut::new() }
    }

    fn feed(&mut self, chunk: &[u8]) {
        self.buf.extend_from_slice(chunk);
    }

    fn next_line(&mut self) -> Option<Bytes> {
        let end = self.buf.iter().position(|byte| *byte == b'\n')?;
        let mut line = self.buf.split_to(end + 1);
        line.truncate(end);
        Some(line.freeze())
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
