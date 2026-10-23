use std::fmt::Write as _;

pub const EDITED: u8 = 1;
pub const REPLY: u8 = 2;
pub const SILENT: u8 = 4;
pub const MAX_TEXT: usize = 4096;
const KNOWN: u8 = EDITED | REPLY | SILENT;

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub flags: u8,
    pub reply_to: Option<u32>,
    pub text: String,
}

#[derive(Debug, PartialEq)]
pub enum EncodeError {
    UnknownFlags(u8),
    ReplyMismatch,
    TooLong(usize),
}

#[derive(Debug, PartialEq)]
pub enum DecodeError {
    Incomplete { needed: usize },
    UnknownFlags(u8),
    TooLong(usize),
    BadUtf8,
}

pub fn encode(msg: &Message) -> Result<Vec<u8>, EncodeError> {
    todo!("флаги, длина, reply_to, текст")
}

pub fn decode(buf: &[u8]) -> Result<(Message, usize), DecodeError> {
    todo!("разбор одного сообщения из начала буфера")
}

#[derive(Default)]
pub struct Stream {
    buf: Vec<u8>,
}

impl Stream {
    pub fn new() -> Stream {
        Stream::default()
    }

    pub fn feed(&mut self, chunk: &[u8]) {
        todo!("добавить байты в буфер")
    }

    pub fn next_message(&mut self) -> Result<Option<Message>, DecodeError> {
        todo!("достать целое сообщение, если оно собралось")
    }

    pub fn buffered(&self) -> usize {
        todo!("сколько байт ждёт в буфере")
    }
}

pub fn hexdump(bytes: &[u8]) -> String {
    todo!("смещение, 16 ячеек, ASCII")
}

pub fn preview(text: &str, max_bytes: usize) -> String {
    todo!("начало текста по границе символа и многоточие")
}
