// HTTP/2 разговаривает не текстом, а бинарными кадрами, и каждый кадр начинается с одинаковых девяти байтов: длина тела
// (24 бита), тип (8), флаги (8) и номер потока (31 бит, старший бит зарезервирован). Через одно TCP-соединение идут десятки
// вызовов, и кадры чужих вызовов перемешаны, поэтому читать их надо точно и осторожно: длину заявляет собеседник.
// TODO: Допиши parse_header, encode_header и split_frames, которая вынимает из буфера все целые кадры и сообщает, сколько байтов
// съедено. Кадр, заявивший тело больше разрешённого, нужно отвергать сразу по заголовку, не дожидаясь тела.

#![allow(dead_code)]

#[derive(Debug, PartialEq, Clone, Copy)]
struct FrameHeader {
    length: u32,
    kind: u8,
    flags: u8,
    stream_id: u32,
}

#[derive(Debug, PartialEq)]
enum HeaderError {
    Short { need: usize },
    TooLong,
}

#[derive(Debug, PartialEq)]
enum SplitError {
    FrameTooLarge { length: u32 },
}

type Frames<'a> = Vec<(FrameHeader, &'a [u8])>;

const HEADER_LEN: usize = 9;

fn parse_header(buf: &[u8]) -> Result<FrameHeader, HeaderError> {
    if buf.len() < HEADER_LEN {
        return Err(HeaderError::Short { need: HEADER_LEN - buf.len() });
    }
    let length = (u32::from(buf[0]) << 16) | (u32::from(buf[1]) << 8) | u32::from(buf[2]);
    let stream_id = u32::from_be_bytes([buf[5], buf[6], buf[7], buf[8]]) & 0x7fff_ffff;
    Ok(FrameHeader { length, kind: buf[3], flags: buf[4], stream_id })
}

fn encode_header(header: &FrameHeader) -> Result<[u8; HEADER_LEN], HeaderError> {
    if header.length > 0x00ff_ffff {
        return Err(HeaderError::TooLong);
    }
    let len = header.length.to_be_bytes();
    let id = (header.stream_id & 0x7fff_ffff).to_be_bytes();
    Ok([len[1], len[2], len[3], header.kind, header.flags, id[0], id[1], id[2], id[3]])
}

fn split_frames(buf: &[u8], max_frame_size: u32) -> Result<(Frames<'_>, usize), SplitError> {
    let mut frames = Vec::new();
    let mut pos = 0;
    while let Ok(header) = parse_header(&buf[pos..]) {
        if header.length > max_frame_size {
            return Err(SplitError::FrameTooLarge { length: header.length });
        }
        let end = pos + HEADER_LEN + header.length as usize;
        if end > buf.len() {
            break;
        }
        frames.push((header, &buf[pos + HEADER_LEN..end]));
        pos = end;
    }
    Ok((frames, pos))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(kind: u8, flags: u8, stream: u32, body: &[u8]) -> Vec<u8> {
        let mut out = encode_header(&FrameHeader { length: body.len() as u32, kind, flags, stream_id: stream }).unwrap().to_vec();
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn header_layout() {
        let bytes = [0x00, 0x01, 0x2c, 0x01, 0x05, 0x00, 0x00, 0x00, 0x03];
        assert_eq!(parse_header(&bytes), Ok(FrameHeader { length: 300, kind: 1, flags: 5, stream_id: 3 }));
        assert_eq!(encode_header(&FrameHeader { length: 300, kind: 1, flags: 5, stream_id: 3 }), Ok(bytes), "длина занимает три байта, номер потока четыре, оба в big-endian");
    }

    #[test]
    fn reserved_bit_is_ignored_on_read_and_cleared_on_write() {
        let with_bit = [0, 0, 0, 0, 0, 0x80, 0, 0, 0x07];
        assert_eq!(parse_header(&with_bit).unwrap().stream_id, 7, "старший бит номера потока зарезервирован и читается как ноль");
        let written = encode_header(&FrameHeader { length: 0, kind: 0, flags: 0, stream_id: 0xffff_ffff }).unwrap();
        assert_eq!(&written[5..], &[0x7f, 0xff, 0xff, 0xff]);
    }

    #[test]
    fn length_limits() {
        let max = FrameHeader { length: 0x00ff_ffff, kind: 0, flags: 0, stream_id: 1 };
        let bytes = encode_header(&max).unwrap();
        assert_eq!(&bytes[..3], &[0xff, 0xff, 0xff]);
        assert_eq!(parse_header(&bytes), Ok(max));
        assert_eq!(encode_header(&FrameHeader { length: 0x0100_0000, ..max }), Err(HeaderError::TooLong), "в 24 бита длина свыше 16 777 215 не влезает");
    }

    #[test]
    fn short_header_says_how_many_bytes_are_missing() {
        assert_eq!(parse_header(&[]), Err(HeaderError::Short { need: 9 }));
        assert_eq!(parse_header(&[0; 4]), Err(HeaderError::Short { need: 5 }));
        assert_eq!(parse_header(&[0; 8]), Err(HeaderError::Short { need: 1 }));
    }

    #[test]
    fn several_frames_in_one_buffer() {
        let mut buf = frame(4, 0, 0, &[0, 3, 0, 0, 0, 100]);
        buf.extend(frame(1, 4, 1, b"hdr"));
        buf.extend(frame(0, 1, 1, b""));
        let (frames, used) = split_frames(&buf, 16384).unwrap();
        assert_eq!(used, buf.len());
        let kinds: Vec<u8> = frames.iter().map(|(h, _)| h.kind).collect();
        assert_eq!(kinds, vec![4, 1, 0]);
        assert_eq!(frames[1].1, b"hdr");
        assert_eq!(frames[2].1, b"", "пустое тело кадра тоже кадр");
    }

    #[test]
    fn a_partial_frame_stays_in_the_buffer() {
        let mut buf = frame(0, 0, 1, b"first");
        let second = frame(0, 1, 3, b"second body");
        buf.extend_from_slice(&second);
        for cut in 0..=second.len() {
            let part = &buf[..buf.len() - second.len() + cut];
            let (frames, used) = split_frames(part, 16384).unwrap();
            if cut == second.len() {
                assert_eq!((frames.len(), used), (2, buf.len()));
            } else {
                assert_eq!((frames.len(), used), (1, 14), "при обрезке второго кадра на {cut} байтах должен вернуться ровно первый кадр, а недочитанное остаться в буфере");
            }
        }
    }

    #[test]
    fn oversized_length_is_rejected_by_the_header_alone() {
        let header = encode_header(&FrameHeader { length: 5_000_000, kind: 0, flags: 0, stream_id: 1 }).unwrap();
        assert_eq!(split_frames(&header, 16384), Err(SplitError::FrameTooLarge { length: 5_000_000 }), "ждать пять мегабайтов тела ради отказа не нужно");
        let ok = encode_header(&FrameHeader { length: 16384, kind: 0, flags: 0, stream_id: 1 }).unwrap();
        assert_eq!(split_frames(&ok, 16384), Ok((vec![], 0)), "ровно по лимиту это допустимо, ждём тело");
    }

    #[test]
    fn byte_by_byte_feed_gives_the_same_frames() {
        let mut stream = Vec::new();
        for n in 0..20u8 {
            stream.extend(frame(n % 3, n, u32::from(n) * 2 + 1, &vec![n; usize::from(n)]));
        }
        let mut buf: Vec<u8> = Vec::new();
        let mut seen = 0;
        for &byte in &stream {
            buf.push(byte);
            let (frames, used) = split_frames(&buf, 16384).unwrap();
            seen += frames.len();
            buf.drain(..used);
        }
        assert_eq!(seen, 20);
        assert!(buf.is_empty());
    }
}
