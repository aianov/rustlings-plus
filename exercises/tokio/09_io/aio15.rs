// Бинарное сообщение: байт вида, четыре байта номера (big-endian), два байта длины тела
// (big-endian), потом тело. Руками вырезать числа индексами скучно и опасно: ошибёшься в
// смещении, и всё поедет. В `bytes` для этого есть `BufMut::put_*` и `Buf::get_*`.
// TODO: Допиши `encode` и `decode`. Декодер вызывают на буфере, куда сеть доливает данные
// кусками, поэтому если сообщение пришло не целиком, он обязан вернуть `None` и оставить буфер
// как был. Цельное сообщение он забирает из буфера, следующее остаётся ждать.
use bytes::{Buf, BufMut, Bytes, BytesMut};

#[derive(Debug, PartialEq, Clone)]
struct Msg {
    kind: u8,
    id: u32,
    body: Vec<u8>,
}

fn encode(msg: &Msg) -> Bytes {
    let _ = msg;
    todo!()
}

fn decode(buf: &mut BytesMut) -> Option<Msg> {
    let _ = buf;
    todo!()
}

fn main() {
    let msg = Msg { kind: 1, id: 7, body: b"hi".to_vec() };
    println!("{:?}", encode(&msg));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(kind: u8, id: u32, body: &[u8]) -> Msg {
        Msg { kind, id, body: body.to_vec() }
    }

    #[test]
    fn wire_format() {
        let bytes = encode(&msg(9, 0x0102_0304, b"ab"));
        assert_eq!(&bytes[..], &[9, 1, 2, 3, 4, 0, 2, b'a', b'b'], "формат: вид, id (4 байта big-endian), длина (2 байта), тело");
    }

    #[test]
    fn roundtrip_two_messages_in_one_buffer() {
        let mut buf = BytesMut::new();
        buf.extend_from_slice(&encode(&msg(1, 10, b"first")));
        buf.extend_from_slice(&encode(&msg(2, 11, b"")));
        assert_eq!(decode(&mut buf), Some(msg(1, 10, b"first")), "первое сообщение");
        assert_eq!(decode(&mut buf), Some(msg(2, 11, b"")), "второе сообщение, с пустым телом");
        assert_eq!(decode(&mut buf), None, "буфер пуст: сообщений больше нет");
    }

    #[test]
    fn partial_input_is_left_alone() {
        let full = encode(&msg(3, 77, b"payload"));
        let mut buf = BytesMut::new();
        for (i, byte) in full.iter().enumerate() {
            assert_eq!(decode(&mut buf), None, "после {i} байт сообщение ещё неполное");
            assert_eq!(buf.len(), i, "неполное сообщение декодер не должен трогать");
            buf.put_u8(*byte);
        }
        assert_eq!(decode(&mut buf), Some(msg(3, 77, b"payload")), "собралось целиком");
        assert!(buf.is_empty(), "цельное сообщение забирается из буфера полностью");
    }
}
