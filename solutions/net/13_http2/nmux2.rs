// У каждого типа кадра свой устройство тела и свои правила, и нарушение многих из них обязывает закрыть соединение с кодом
// ошибки. DATA несёт кусок тела вызова, HEADERS блок заголовков, RST_STREAM обрывает один вызов, PING проверяет связь,
// GOAWAY просит всех расходиться, WINDOW_UPDATE выдаёт кредит на отправку. Часть тел дополнена выравниванием (PADDED) и приоритетом.
// TODO: Допиши decode_frame, которая превращает заголовок и тело в Frame или в ошибку соединения. Неизвестные типы кадров
// не ошибка, а Frame::Unknown: их положено молча пропускать. Правила проверки смотри в тестах.

#![allow(dead_code)]

#[derive(Debug, PartialEq, Clone, Copy)]
struct FrameHeader {
    length: u32,
    kind: u8,
    flags: u8,
    stream_id: u32,
}

#[derive(Debug, PartialEq)]
enum Frame {
    Data { stream: u32, data: Vec<u8>, end_stream: bool },
    Headers { stream: u32, block: Vec<u8>, end_stream: bool, end_headers: bool },
    Priority { stream: u32 },
    RstStream { stream: u32, code: u32 },
    Settings { ack: bool, entries: Vec<(u16, u32)> },
    Ping { ack: bool, data: [u8; 8] },
    Goaway { last_stream: u32, code: u32, debug: Vec<u8> },
    WindowUpdate { stream: u32, increment: u32 },
    Continuation { stream: u32, block: Vec<u8>, end_headers: bool },
    Unknown { kind: u8 },
}

const PROTOCOL_ERROR: u32 = 1;
const FRAME_SIZE_ERROR: u32 = 6;

#[derive(Debug, PartialEq)]
struct ConnError(u32);

const END_STREAM: u8 = 0x1;
const ACK: u8 = 0x1;
const END_HEADERS: u8 = 0x4;
const PADDED: u8 = 0x8;
const PRIORITY: u8 = 0x20;

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

fn decode_frame(h: FrameHeader, body: &[u8]) -> Result<Frame, ConnError> {
    let stream = h.stream_id;
    let need_stream = || if stream == 0 { Err(ConnError(PROTOCOL_ERROR)) } else { Ok(()) };
    let need_no_stream = || if stream != 0 { Err(ConnError(PROTOCOL_ERROR)) } else { Ok(()) };
    let exact = |n: usize| if body.len() == n { Ok(()) } else { Err(ConnError(FRAME_SIZE_ERROR)) };
    let unpad = |mut data: &[u8]| -> Result<Vec<u8>, ConnError> {
        let mut tail = 0;
        if h.flags & PADDED != 0 {
            let (&pad, rest) = data.split_first().ok_or(ConnError(FRAME_SIZE_ERROR))?;
            if usize::from(pad) >= data.len() {
                return Err(ConnError(PROTOCOL_ERROR));
            }
            data = rest;
            tail = usize::from(pad);
        }
        Ok(data[..data.len() - tail].to_vec())
    };
    match h.kind {
        0 => {
            need_stream()?;
            Ok(Frame::Data { stream, data: unpad(body)?, end_stream: h.flags & END_STREAM != 0 })
        }
        1 => {
            need_stream()?;
            let mut block = unpad(body)?;
            if h.flags & PRIORITY != 0 {
                if block.len() < 5 {
                    return Err(ConnError(FRAME_SIZE_ERROR));
                }
                block.drain(..5);
            }
            Ok(Frame::Headers { stream, block, end_stream: h.flags & END_STREAM != 0, end_headers: h.flags & END_HEADERS != 0 })
        }
        2 => {
            need_stream()?;
            exact(5)?;
            Ok(Frame::Priority { stream })
        }
        3 => {
            need_stream()?;
            exact(4)?;
            Ok(Frame::RstStream { stream, code: be32(body) })
        }
        4 => {
            need_no_stream()?;
            let ack = h.flags & ACK != 0;
            if body.len() % 6 != 0 || (ack && !body.is_empty()) {
                return Err(ConnError(FRAME_SIZE_ERROR));
            }
            let entries = body.chunks(6).map(|c| (u16::from_be_bytes([c[0], c[1]]), be32(&c[2..]))).collect();
            Ok(Frame::Settings { ack, entries })
        }
        6 => {
            need_no_stream()?;
            exact(8)?;
            Ok(Frame::Ping { ack: h.flags & ACK != 0, data: body.try_into().unwrap() })
        }
        7 => {
            need_no_stream()?;
            if body.len() < 8 {
                return Err(ConnError(FRAME_SIZE_ERROR));
            }
            Ok(Frame::Goaway { last_stream: be32(body) & 0x7fff_ffff, code: be32(&body[4..]), debug: body[8..].to_vec() })
        }
        8 => {
            exact(4)?;
            let increment = be32(body) & 0x7fff_ffff;
            if increment == 0 {
                return Err(ConnError(PROTOCOL_ERROR));
            }
            Ok(Frame::WindowUpdate { stream, increment })
        }
        9 => {
            need_stream()?;
            Ok(Frame::Continuation { stream, block: body.to_vec(), end_headers: h.flags & END_HEADERS != 0 })
        }
        kind => Ok(Frame::Unknown { kind }),
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(kind: u8, flags: u8, stream_id: u32, body: &[u8]) -> (FrameHeader, Vec<u8>) {
        (FrameHeader { length: body.len() as u32, kind, flags, stream_id }, body.to_vec())
    }

    fn decode(kind: u8, flags: u8, stream: u32, body: &[u8]) -> Result<Frame, ConnError> {
        let (header, body) = h(kind, flags, stream, body);
        decode_frame(header, &body)
    }

    #[test]
    fn data_and_headers_flags() {
        assert_eq!(decode(0, 0, 1, b"abc"), Ok(Frame::Data { stream: 1, data: b"abc".to_vec(), end_stream: false }));
        assert_eq!(decode(0, END_STREAM, 3, b""), Ok(Frame::Data { stream: 3, data: vec![], end_stream: true }));
        assert_eq!(decode(1, END_HEADERS, 1, b"blk"), Ok(Frame::Headers { stream: 1, block: b"blk".to_vec(), end_stream: false, end_headers: true }));
        assert_eq!(decode(1, END_HEADERS | END_STREAM, 5, b""), Ok(Frame::Headers { stream: 5, block: vec![], end_stream: true, end_headers: true }));
        assert_eq!(decode(1, 0, 1, b"half"), Ok(Frame::Headers { stream: 1, block: b"half".to_vec(), end_stream: false, end_headers: false }));
        assert_eq!(decode(9, END_HEADERS, 1, b"more"), Ok(Frame::Continuation { stream: 1, block: b"more".to_vec(), end_headers: true }));
    }

    #[test]
    fn padded_data_and_headers() {
        assert_eq!(decode(0, PADDED, 1, &[2, b'h', b'i', 0, 0]), Ok(Frame::Data { stream: 1, data: b"hi".to_vec(), end_stream: false }), "выравнивание: первый байт длина, нули в конце не часть данных");
        assert_eq!(decode(0, PADDED | END_STREAM, 1, &[0, b'x']), Ok(Frame::Data { stream: 1, data: b"x".to_vec(), end_stream: true }), "выравнивание нулевой длины допустимо");
        assert_eq!(decode(0, PADDED, 1, &[4, b'a', 0, 0]), Err(ConnError(PROTOCOL_ERROR)), "длина выравнивания не меньше длины тела");
        assert_eq!(decode(0, PADDED, 1, &[3, b'a', 0, 0]), Ok(Frame::Data { stream: 1, data: vec![], end_stream: false }), "выравнивание в три байта занимает всё после байта длины");
        assert_eq!(decode(0, PADDED, 1, &[1, 0]), Ok(Frame::Data { stream: 1, data: vec![], end_stream: false }));
        assert_eq!(decode(0, PADDED, 1, &[1]), Err(ConnError(PROTOCOL_ERROR)), "выравнивание не может занять всё тело");
        assert_eq!(decode(0, PADDED, 1, &[]), Err(ConnError(FRAME_SIZE_ERROR)), "флаг есть, а байта длины нет");
        assert_eq!(decode(1, PADDED | END_HEADERS, 1, &[1, b'b', 0]), Ok(Frame::Headers { stream: 1, block: b"b".to_vec(), end_stream: false, end_headers: true }));
    }

    #[test]
    fn headers_with_priority_fields_skip_them() {
        let body = [0, 0, 0, 0, 15, b'h', b'd'];
        assert_eq!(decode(1, END_HEADERS | PRIORITY, 1, &body), Ok(Frame::Headers { stream: 1, block: b"hd".to_vec(), end_stream: false, end_headers: true }));
        let padded = [1, 0, 0, 0, 0, 15, b'h', b'd', 0];
        assert_eq!(decode(1, END_HEADERS | PRIORITY | PADDED, 1, &padded), Ok(Frame::Headers { stream: 1, block: b"hd".to_vec(), end_stream: false, end_headers: true }), "сначала снимается выравнивание, потом приоритет");
        assert_eq!(decode(1, PRIORITY, 1, &[0, 0, 0]), Err(ConnError(FRAME_SIZE_ERROR)));
    }

    #[test]
    fn stream_zero_rules() {
        for kind in [0, 1, 2, 3, 9] {
            assert_eq!(decode(kind, 0, 0, &[0; 5]), Err(ConnError(PROTOCOL_ERROR)), "кадр типа {kind} обязан относиться к потоку");
        }
        for (kind, body) in [(4u8, vec![]), (6, vec![0; 8]), (7, vec![0; 8])] {
            assert!(decode(kind, 0, 0, &body).is_ok());
            assert_eq!(decode(kind, 0, 1, &body), Err(ConnError(PROTOCOL_ERROR)), "кадр типа {kind} относится к соединению, номер потока обязан быть нулём");
        }
        assert!(decode(8, 0, 0, &[0, 0, 0, 1]).is_ok(), "WINDOW_UPDATE бывает и для соединения");
        assert!(decode(8, 0, 5, &[0, 0, 0, 1]).is_ok());
    }

    #[test]
    fn fixed_size_frames() {
        assert_eq!(decode(3, 0, 1, &[0, 0, 0, 8]), Ok(Frame::RstStream { stream: 1, code: 8 }));
        assert_eq!(decode(3, 0, 1, &[0, 0, 0]), Err(ConnError(FRAME_SIZE_ERROR)));
        assert_eq!(decode(6, 0, 0, &[1, 2, 3, 4, 5, 6, 7, 8]), Ok(Frame::Ping { ack: false, data: [1, 2, 3, 4, 5, 6, 7, 8] }));
        assert_eq!(decode(6, ACK, 0, &[0; 8]), Ok(Frame::Ping { ack: true, data: [0; 8] }));
        assert_eq!(decode(6, 0, 0, &[0; 7]), Err(ConnError(FRAME_SIZE_ERROR)));
        assert_eq!(decode(2, 0, 3, &[0; 5]), Ok(Frame::Priority { stream: 3 }));
        assert_eq!(decode(2, 0, 3, &[0; 4]), Err(ConnError(FRAME_SIZE_ERROR)));
    }

    #[test]
    fn window_update_rules() {
        assert_eq!(decode(8, 0, 0, &[0, 1, 0, 0]), Ok(Frame::WindowUpdate { stream: 0, increment: 65536 }));
        assert_eq!(decode(8, 0, 1, &[0x80, 0, 0, 5]), Ok(Frame::WindowUpdate { stream: 1, increment: 5 }), "старший бит зарезервирован");
        assert_eq!(decode(8, 0, 1, &[0, 0, 0, 0]), Err(ConnError(PROTOCOL_ERROR)), "нулевой кредит это нарушение");
        assert_eq!(decode(8, 0, 1, &[0, 0, 5]), Err(ConnError(FRAME_SIZE_ERROR)));
    }

    #[test]
    fn settings_goaway_and_unknown() {
        assert_eq!(decode(4, 0, 0, &[0, 3, 0, 0, 0, 100, 0, 4, 0, 1, 0, 0]), Ok(Frame::Settings { ack: false, entries: vec![(3, 100), (4, 65536)] }));
        assert_eq!(decode(4, ACK, 0, &[]), Ok(Frame::Settings { ack: true, entries: vec![] }));
        assert_eq!(decode(4, ACK, 0, &[0; 6]), Err(ConnError(FRAME_SIZE_ERROR)), "подтверждение настроек обязано быть пустым");
        assert_eq!(decode(4, 0, 0, &[0; 7]), Err(ConnError(FRAME_SIZE_ERROR)), "настройки приходят по шесть байтов");
        let goaway = [0x80, 0, 0, 9, 0, 0, 0, 2, b'b', b'y', b'e'];
        assert_eq!(decode(7, 0, 0, &goaway), Ok(Frame::Goaway { last_stream: 9, code: 2, debug: b"bye".to_vec() }));
        assert_eq!(decode(7, 0, 0, &goaway[..7]), Err(ConnError(FRAME_SIZE_ERROR)));
        assert_eq!(decode(0xfe, 0xff, 0, b"anything"), Ok(Frame::Unknown { kind: 0xfe }), "неизвестные типы кадров игнорируются, даже с флагами и на потоке ноль");
        assert_eq!(decode(0xfe, 0, 7, b""), Ok(Frame::Unknown { kind: 0xfe }));
    }

    #[test]
    fn undefined_flags_are_ignored() {
        assert_eq!(decode(0, 0xf6, 1, b"x"), Ok(Frame::Data { stream: 1, data: b"x".to_vec(), end_stream: false }), "флаги, которых нет у DATA, читатель не трогает");
    }
}
