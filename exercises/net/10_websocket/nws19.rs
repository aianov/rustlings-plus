// Закрытие соединения в WebSocket это разговор из двух реплик: кто хочет закончить, присылает Close с кодом и причиной,
// другая сторона отвечает своим Close, и после этого сервер закрывает TCP. Так обе стороны знают, что оборвало не
// сеть, а решение. Если ответа нет, ждать вечно нельзя: у ожидания есть общий срок.
// TODO: Допиши разбор и сборку тела Close, автомат Closing (кто что отправляет, когда данные уже не принимаются)
// и wait_for_close_reply: ждём Close не дольше limit в сумме, остальные фреймы по дороге выбрасываем.

#![allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opcode {
    Continuation,
    Text,
    Binary,
    Close,
    Ping,
    Pong,
}

impl Opcode {
    fn from_u8(value: u8) -> Option<Opcode> {
        match value {
            0x0 => Some(Opcode::Continuation),
            0x1 => Some(Opcode::Text),
            0x2 => Some(Opcode::Binary),
            0x8 => Some(Opcode::Close),
            0x9 => Some(Opcode::Ping),
            0xA => Some(Opcode::Pong),
            _ => None,
        }
    }

    fn to_u8(self) -> u8 {
        match self {
            Opcode::Continuation => 0x0,
            Opcode::Text => 0x1,
            Opcode::Binary => 0x2,
            Opcode::Close => 0x8,
            Opcode::Ping => 0x9,
            Opcode::Pong => 0xA,
        }
    }

    fn is_control(self) -> bool {
        self.to_u8() & 0x8 != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProtoError {
    ReservedBits,
    BadOpcode(u8),
    NonMinimalLength,
    LengthHighBit,
    MaskRequired,
    MaskForbidden,
    TooBig,
    FragmentedControl,
    ControlTooLong,
    UnexpectedContinuation,
    ExpectedContinuation,
    InvalidUtf8,
    BadClose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Server,
    Client,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Frame {
    fin: bool,
    opcode: Opcode,
    payload: Vec<u8>,
}

impl Frame {
    fn new(opcode: Opcode, payload: &[u8]) -> Frame {
        Frame { fin: true, opcode, payload: payload.to_vec() }
    }

    fn text(s: &str) -> Frame {
        Frame::new(Opcode::Text, s.as_bytes())
    }

    fn part(fin: bool, opcode: Opcode, payload: &[u8]) -> Frame {
        Frame { fin, opcode, payload: payload.to_vec() }
    }
}

use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Open,
    Closing,
    Closed,
}

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Replied,
    TimedOut,
    Dropped,
}

fn valid_close_code(code: u16) -> bool {
    let _ = code;
    todo!()
}

fn parse_close(payload: &[u8]) -> Result<Option<(u16, String)>, ProtoError> {
    let _ = payload;
    todo!()
}

fn encode_close(code: u16, reason: &str) -> Vec<u8> {
    let _ = (code, reason);
    todo!()
}

struct Closing {
    phase: Phase,
}

impl Closing {
    fn new() -> Closing {
        Closing { phase: Phase::Open }
    }

    fn accepts_data(&self) -> bool {
        self.phase == Phase::Open
    }

    fn start(&mut self, code: u16, reason: &str) -> Option<Frame> {
        let _ = (code, reason);
        todo!()
    }

    fn on_close(&mut self, payload: &[u8]) -> Option<Frame> {
        let _ = payload;
        todo!()
    }
}

async fn wait_for_close_reply(rx: &mut mpsc::Receiver<Frame>, limit: Duration) -> Outcome {
    let _ = (rx, limit);
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::time::sleep;

    #[test]
    fn parse_close_bodies() {
        assert_eq!(parse_close(&[]), Ok(None), "пустое тело разрешено: код не указан");
        assert_eq!(parse_close(&[0x03, 0xe8]), Ok(Some((1000, String::new()))));
        assert_eq!(parse_close(&[0x03, 0xe9, b'b', b'y', b'e']), Ok(Some((1001, "bye".to_string()))));
        assert_eq!(parse_close(&[0x0f, 0xa0, 0xd0, 0xbf]), Ok(Some((4000, "п".to_string()))), "коды 3000..4999 свободны для приложений");
    }

    #[test]
    fn parse_close_errors() {
        assert_eq!(parse_close(&[0x03]), Err(ProtoError::BadClose), "один байт это не код");
        for code in [0u16, 999, 1004, 1005, 1006, 1015, 1016, 2999, 5000, 65535] {
            let body = code.to_be_bytes();
            assert_eq!(parse_close(&body), Err(ProtoError::BadClose), "код {code} нельзя передавать по сети");
        }
        assert_eq!(parse_close(&[0x03, 0xe8, 0xff]), Err(ProtoError::InvalidUtf8), "причина должна быть UTF-8");
    }

    #[test]
    fn valid_codes() {
        for code in [1000u16, 1001, 1002, 1003, 1007, 1008, 1009, 1010, 1011, 3000, 4999] {
            assert!(valid_close_code(code), "{code} допустим");
        }
    }

    #[test]
    fn encode_close_basic_and_truncation() {
        assert_eq!(encode_close(1000, ""), vec![0x03, 0xe8]);
        assert_eq!(encode_close(1001, "bye"), vec![0x03, 0xe9, b'b', b'y', b'e']);
        let long = "я".repeat(100);
        let body = encode_close(1000, &long);
        assert!(body.len() <= 125, "тело управляющего фрейма не длиннее 125 байт, а у нас {}", body.len());
        assert!(std::str::from_utf8(&body[2..]).is_ok(), "обрезать причину надо по границе символа, иначе получится не UTF-8");
        assert_eq!(body.len(), 2 + 122, "от `я` (два байта) влезает 61 штука");
        let ascii = encode_close(1000, &"x".repeat(500));
        assert_eq!(ascii.len(), 125);
    }

    #[test]
    fn we_start_the_close() {
        let mut c = Closing::new();
        assert!(c.accepts_data());
        let frame = c.start(1000, "done").expect("начало закрытия отправляет Close");
        assert_eq!(frame.opcode, Opcode::Close);
        assert!(frame.fin);
        assert_eq!(frame.payload, encode_close(1000, "done"));
        assert_eq!(c.phase, Phase::Closing);
        assert!(!c.accepts_data(), "после отправки Close данные больше не отправляют и не принимают");
        assert!(c.start(1001, "again").is_none(), "второй Close не отправляют");
        assert!(c.on_close(&encode_close(1000, "")).is_none(), "ответ на наш Close ответа не требует");
        assert_eq!(c.phase, Phase::Closed);
    }

    #[test]
    fn peer_starts_the_close() {
        let mut c = Closing::new();
        let reply = c.on_close(&encode_close(1001, "going away")).expect("на Close отвечаем Close");
        assert_eq!(reply.opcode, Opcode::Close);
        assert_eq!(reply.payload, encode_close(1001, ""), "отвечаем тем же кодом");
        assert_eq!(c.phase, Phase::Closed);
        assert!(c.on_close(&[]).is_none(), "второй Close не получает ответа");
        assert!(c.start(1000, "").is_none());
    }

    #[test]
    fn peer_close_without_code() {
        let mut c = Closing::new();
        let reply = c.on_close(&[]).unwrap();
        assert!(reply.payload.is_empty(), "кода не было, ответ тоже без кода");
    }

    #[test]
    fn broken_close_bodies_are_answered_with_error_codes() {
        let mut c = Closing::new();
        assert_eq!(c.on_close(&[0x03]).unwrap().payload, encode_close(1002, ""));
        let mut c = Closing::new();
        assert_eq!(c.on_close(&[0x03, 0xe8, 0xff]).unwrap().payload, encode_close(1007, ""), "причина не UTF-8");
        let mut c = Closing::new();
        assert_eq!(c.on_close(&[0x03, 0xec]).unwrap().payload, encode_close(1002, ""), "код 1004 запрещён");
    }

    fn data() -> Frame {
        Frame::text("data")
    }

    #[tokio::test(start_paused = true)]
    async fn the_reply_arrives_in_time_among_other_frames() {
        let (tx, mut rx) = mpsc::channel(8);
        tokio::spawn(async move {
            tx.send(data()).await.unwrap();
            sleep(Duration::from_secs(2)).await;
            tx.send(Frame::new(Opcode::Ping, b"")).await.unwrap();
            tx.send(Frame::new(Opcode::Close, &encode_close(1000, ""))).await.unwrap();
        });
        let started = Instant::now();
        assert_eq!(wait_for_close_reply(&mut rx, Duration::from_secs(5)).await, Outcome::Replied);
        assert!(started.elapsed() < Duration::from_secs(3), "дождались ответа за {:?}", started.elapsed());
    }

    #[tokio::test(start_paused = true)]
    async fn silence_ends_with_a_timeout() {
        let (_tx, mut rx) = mpsc::channel::<Frame>(8);
        let started = Instant::now();
        assert_eq!(wait_for_close_reply(&mut rx, Duration::from_secs(5)).await, Outcome::TimedOut);
        let waited = started.elapsed();
        assert!(waited >= Duration::from_secs(5) && waited < Duration::from_secs(6), "ждали {waited:?}, срок был 5 секунд");
    }

    #[tokio::test(start_paused = true)]
    async fn a_chatty_peer_cannot_stretch_the_deadline() {
        let (tx, mut rx) = mpsc::channel(8);
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(1)).await;
                if tx.send(data()).await.is_err() {
                    break;
                }
            }
        });
        let started = Instant::now();
        assert_eq!(wait_for_close_reply(&mut rx, Duration::from_secs(5)).await, Outcome::TimedOut);
        let waited = started.elapsed();
        assert!(waited < Duration::from_secs(6), "ждали {waited:?}: срок общий, а не продлевается каждым пришедшим фреймом");
    }

    #[tokio::test(start_paused = true)]
    async fn a_dropped_connection_is_not_a_timeout() {
        let (tx, mut rx) = mpsc::channel::<Frame>(8);
        tx.send(data()).await.unwrap();
        drop(tx);
        let started = Instant::now();
        assert_eq!(wait_for_close_reply(&mut rx, Duration::from_secs(5)).await, Outcome::Dropped);
        assert!(started.elapsed() < Duration::from_secs(1), "канал закрыт: ждать больше нечего");
    }
}
