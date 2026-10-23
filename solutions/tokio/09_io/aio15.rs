use bytes::{Buf, BufMut, Bytes, BytesMut};

#[derive(Debug, PartialEq, Clone)]
struct Msg {
    kind: u8,
    id: u32,
    body: Vec<u8>,
}

const HEADER: usize = 1 + 4 + 2;

fn encode(msg: &Msg) -> Bytes {
    let mut out = BytesMut::with_capacity(HEADER + msg.body.len());
    out.put_u8(msg.kind);
    out.put_u32(msg.id);
    out.put_u16(msg.body.len() as u16);
    out.put_slice(&msg.body);
    out.freeze()
}

fn decode(buf: &mut BytesMut) -> Option<Msg> {
    if buf.len() < HEADER {
        return None;
    }
    let mut peek = &buf[..HEADER];
    let kind = peek.get_u8();
    let id = peek.get_u32();
    let len = peek.get_u16() as usize;
    if buf.len() < HEADER + len {
        return None;
    }
    buf.advance(HEADER);
    let body = buf.split_to(len).to_vec();
    Some(Msg { kind, id, body })
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
