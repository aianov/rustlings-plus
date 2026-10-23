// Управляющие фреймы (ping, pong, close) могут прилететь в любой момент, даже посреди чужого длинного сообщения,
// поэтому у них свои строгие правила: короткие (до 125 байт) и неразрезанные. Это удобно: проверить их можно по
// заголовку, ещё не читая данных. На ping сервер обязан ответить pong с теми же данными, на pong ничего отвечать
// не надо (его могут прислать и без ping, как однонаправленный «я жив»).
// TODO: Допиши check_control_header (проверка по заголовку) и handle_control (что делать с готовым управляющим
// фреймом). Close пока отдаём наружу как есть, разберём его позже.

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

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Reply(Frame),
    Nothing,
    Close(Vec<u8>),
}

fn check_control_header(opcode: Opcode, fin: bool, declared_len: u64) -> Result<(), ProtoError> {
    let _ = (opcode, fin, declared_len);
    todo!()
}

fn handle_control(frame: Frame) -> Result<Action, ProtoError> {
    let _ = frame;
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_gets_pong_with_the_same_body() {
        assert_eq!(handle_control(Frame::new(Opcode::Ping, b"abc")), Ok(Action::Reply(Frame::new(Opcode::Pong, b"abc"))));
        assert_eq!(handle_control(Frame::new(Opcode::Ping, b"")), Ok(Action::Reply(Frame::new(Opcode::Pong, b""))));
        let body: Vec<u8> = (0..125u8).collect();
        match handle_control(Frame::new(Opcode::Ping, &body)) {
            Ok(Action::Reply(pong)) => {
                assert_eq!(pong.opcode, Opcode::Pong);
                assert!(pong.fin, "pong это целый фрейм");
                assert_eq!(pong.payload, body, "тело pong должно совпасть с телом ping байт в байт, включая максимальные 125 байт");
            }
            other => panic!("на ping ждали ответ pong, а получили {other:?}"),
        }
    }

    #[test]
    fn pong_needs_no_answer() {
        assert_eq!(handle_control(Frame::new(Opcode::Pong, b"x")), Ok(Action::Nothing), "на pong не отвечают: иначе два конца будут вечно пинговать друг друга");
    }

    #[test]
    fn close_is_passed_up() {
        assert_eq!(handle_control(Frame::new(Opcode::Close, &[0x03, 0xe8, b'o', b'k'])), Ok(Action::Close(vec![0x03, 0xe8, b'o', b'k'])));
        assert_eq!(handle_control(Frame::new(Opcode::Close, b"")), Ok(Action::Close(vec![])));
    }

    #[test]
    fn control_frames_cannot_be_fragmented() {
        for op in [Opcode::Ping, Opcode::Pong, Opcode::Close] {
            assert_eq!(handle_control(Frame::part(false, op, b"x")), Err(ProtoError::FragmentedControl), "{op:?} с FIN=0 запрещён");
            assert_eq!(check_control_header(op, false, 1), Err(ProtoError::FragmentedControl));
        }
    }

    #[test]
    fn control_frames_are_short() {
        for op in [Opcode::Ping, Opcode::Pong, Opcode::Close] {
            assert_eq!(handle_control(Frame::new(op, &[0u8; 126])), Err(ProtoError::ControlTooLong), "{op:?} длиннее 125 байт");
            assert_eq!(check_control_header(op, true, 126), Err(ProtoError::ControlTooLong));
            assert_eq!(check_control_header(op, true, 125), Ok(()));
            assert!(handle_control(Frame::new(op, &[0u8; 125])).is_ok());
        }
        assert_eq!(check_control_header(Opcode::Ping, true, 1 << 40), Err(ProtoError::ControlTooLong), "заявленная длина проверяется ещё до получения данных");
    }

    #[test]
    fn data_frames_are_not_control() {
        assert_eq!(check_control_header(Opcode::Text, false, 1 << 30), Ok(()), "у данных свои правила: фрагментация и большие длины для них законны");
        assert_eq!(check_control_header(Opcode::Binary, true, 1 << 30), Ok(()));
        assert!(handle_control(Frame::text("x")).is_err(), "handle_control принимает только управляющие фреймы");
    }
}
