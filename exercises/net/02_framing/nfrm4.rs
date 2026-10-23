// Из сокета пришла пачка байтов: два целых сообщения и половинка третьего. Целые нужно отдать,
// половинку сохранить до следующего чтения, ни байта не потеряв и ни байта не скопировав зря.
// TODO: Допиши take_messages: достань из buf все законченные сообщения (до '\n', без самого
// '\n') в порядке прихода. Недописанный хвост оставь в buf.

use bytes::{Bytes, BytesMut};

fn take_messages(buf: &mut BytesMut) -> Vec<Bytes> {
    todo!()
}

fn main() {
    let mut buf = BytesMut::from(&b"one\ntwo\nthr"[..]);
    let done = take_messages(&mut buf);
    println!("готово {} сообщений, в буфере осталось {:?}", done.len(), buf);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(v: &[Bytes]) -> Vec<String> {
        v.iter().map(|b| String::from_utf8_lossy(b).into_owned()).collect()
    }

    #[test]
    fn whole_messages_and_a_tail() {
        let mut buf = BytesMut::from(&b"ab\ncd\nef"[..]);
        let got = take_messages(&mut buf);
        assert_eq!(texts(&got), ["ab", "cd"], "два законченных сообщения");
        assert_eq!(&buf[..], b"ef", "недописанный хвост должен остаться в буфере");
    }

    #[test]
    fn nothing_complete_yet() {
        let mut buf = BytesMut::from(&b"abc"[..]);
        assert!(take_messages(&mut buf).is_empty(), "'\\n' ещё не пришёл, сообщений нет");
        assert_eq!(&buf[..], b"abc", "всё содержимое должно остаться");
    }

    #[test]
    fn empty_messages_count() {
        let mut buf = BytesMut::from(&b"\n\nx\n"[..]);
        assert_eq!(texts(&take_messages(&mut buf)), ["", "", "x"], "пустые строки это тоже сообщения");
        assert!(buf.is_empty(), "буфер пуст: хвоста нет");
    }

    #[test]
    fn tail_completes_on_the_next_chunk() {
        let mut buf = BytesMut::from(&b"he"[..]);
        assert!(take_messages(&mut buf).is_empty());
        buf.extend_from_slice(b"llo\nwor");
        assert_eq!(texts(&take_messages(&mut buf)), ["hello"], "начало, пришедшее раньше, должно склеиться с продолжением");
        buf.extend_from_slice(b"ld\n");
        assert_eq!(texts(&take_messages(&mut buf)), ["world"]);
    }
}
