// gRPC кладёт каждое сообщение в тело HTTP/2 с собственным пятибайтным префиксом: байт «сжато или нет» и четыре байта длины.
// Границы кадров DATA и границы сообщений между собой никак не связаны: в одном DATA бывает несколько сообщений, а одно сообщение
// может растянуться на несколько DATA. Кроме того, длину снова заявляет собеседник, и серверу нельзя копить гигабайт
// ради сообщения, которое он всё равно отвергнет. TODO: Допиши encode_message и Decoder::feed: он принимает очередную порцию
// байтов и возвращает все целые сообщения, накопленные к этому моменту, а недочитанное запоминает.

#![allow(dead_code)]

#[derive(Debug, PartialEq)]
enum GrpcError {
    Compressed,
    BadFlag(u8),
    TooLarge { length: usize },
}

fn encode_message(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + payload.len());
    out.push(0);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

struct Decoder {
    buf: Vec<u8>,
    max_len: usize,
}

impl Decoder {
    fn new(max_len: usize) -> Decoder {
        Decoder { buf: Vec::new(), max_len }
    }

    fn buffered(&self) -> usize {
        self.buf.len()
    }

    fn feed(&mut self, chunk: &[u8]) -> Result<Vec<Vec<u8>>, GrpcError> {
        self.buf.extend_from_slice(chunk);
        let mut messages = Vec::new();
        let mut pos = 0;
        while self.buf.len() - pos >= 5 {
            let flag = self.buf[pos];
            let length = u32::from_be_bytes([self.buf[pos + 1], self.buf[pos + 2], self.buf[pos + 3], self.buf[pos + 4]]) as usize;
            match flag {
                0 => {}
                1 => return Err(GrpcError::Compressed),
                other => return Err(GrpcError::BadFlag(other)),
            }
            if length > self.max_len {
                return Err(GrpcError::TooLarge { length });
            }
            if self.buf.len() - pos - 5 < length {
                break;
            }
            messages.push(self.buf[pos + 5..pos + 5 + length].to_vec());
            pos += 5 + length;
        }
        self.buf.drain(..pos);
        Ok(messages)
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_layout() {
        assert_eq!(encode_message(b"hello"), vec![0, 0, 0, 0, 5, b'h', b'e', b'l', b'l', b'o']);
        assert_eq!(encode_message(b""), vec![0, 0, 0, 0, 0], "пустое сообщение это один пятибайтный префикс");
        let big = encode_message(&vec![7u8; 300]);
        assert_eq!(&big[..5], &[0, 0, 0, 1, 44], "длина четыре байта, big-endian");
    }

    #[test]
    fn one_message_per_chunk() {
        let mut d = Decoder::new(1 << 20);
        assert_eq!(d.feed(&encode_message(b"abc")), Ok(vec![b"abc".to_vec()]));
        assert_eq!(d.feed(&encode_message(b"")), Ok(vec![vec![]]));
        assert_eq!(d.buffered(), 0);
    }

    #[test]
    fn two_messages_in_one_chunk() {
        let mut d = Decoder::new(1 << 20);
        let mut chunk = encode_message(b"first");
        chunk.extend(encode_message(b"second"));
        chunk.extend(encode_message(b""));
        assert_eq!(d.feed(&chunk), Ok(vec![b"first".to_vec(), b"second".to_vec(), vec![]]));
    }

    #[test]
    fn one_message_split_across_chunks_at_every_point() {
        let message = encode_message(b"split me please");
        for cut in 1..message.len() {
            let mut d = Decoder::new(1 << 20);
            assert_eq!(d.feed(&message[..cut]), Ok(vec![]), "на {cut} байтах сообщение ещё не готово");
            assert_eq!(d.buffered(), cut);
            assert_eq!(d.feed(&message[cut..]), Ok(vec![b"split me please".to_vec()]), "разрез на {cut}");
            assert_eq!(d.buffered(), 0);
        }
    }

    #[test]
    fn byte_by_byte_with_several_messages() {
        let mut stream = Vec::new();
        for n in 0..10u8 {
            stream.extend(encode_message(&vec![n; usize::from(n) * 3]));
        }
        let mut d = Decoder::new(1 << 20);
        let mut got = Vec::new();
        for &b in &stream {
            got.extend(d.feed(&[b]).unwrap());
        }
        assert_eq!(got.len(), 10);
        assert_eq!(got[9], vec![9u8; 27]);
        assert_eq!(d.buffered(), 0);
    }

    #[test]
    fn rest_stays_after_a_complete_message() {
        let mut d = Decoder::new(1 << 20);
        let mut chunk = encode_message(b"done");
        chunk.extend_from_slice(&encode_message(b"next")[..4]);
        assert_eq!(d.feed(&chunk), Ok(vec![b"done".to_vec()]));
        assert_eq!(d.buffered(), 4, "четыре байта следующего префикса остались ждать");
        assert_eq!(d.feed(&encode_message(b"next")[4..]), Ok(vec![b"next".to_vec()]));
    }

    #[test]
    fn compressed_flag_without_negotiated_encoding_is_an_error() {
        let mut d = Decoder::new(1 << 20);
        assert_eq!(d.feed(&[1, 0, 0, 0, 1, 9]), Err(GrpcError::Compressed));
        let mut d = Decoder::new(1 << 20);
        assert_eq!(d.feed(&[2, 0, 0, 0, 0]), Err(GrpcError::BadFlag(2)));
    }

    #[test]
    fn oversized_declaration_is_rejected_before_the_body_arrives() {
        let mut d = Decoder::new(1000);
        assert_eq!(d.feed(&[0, 0, 0, 0x27, 0x10]), Err(GrpcError::TooLarge { length: 10_000 }), "по префиксу уже ясно, что 10000 больше лимита 1000: тела ждать не надо");
        let mut d = Decoder::new(1000);
        assert_eq!(d.feed(&[0, 0xff, 0xff, 0xff, 0xff]), Err(GrpcError::TooLarge { length: 0xffff_ffff }));
        let mut d = Decoder::new(1000);
        assert!(d.feed(&encode_message(&vec![0u8; 1000])).is_ok(), "ровно по лимиту это допустимо");
    }

    #[test]
    fn flag_is_checked_as_soon_as_it_arrives() {
        let mut d = Decoder::new(1 << 20);
        assert_eq!(d.feed(&[1, 0, 0, 0, 4]), Err(GrpcError::Compressed), "тело не нужно, чтобы понять, что сообщение сжато");
    }
}
