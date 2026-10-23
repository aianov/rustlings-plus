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
    if msg.flags & !KNOWN != 0 {
        return Err(EncodeError::UnknownFlags(msg.flags));
    }
    if (msg.flags & REPLY != 0) != msg.reply_to.is_some() {
        return Err(EncodeError::ReplyMismatch);
    }
    let len = msg.text.len();
    let wire_len = u16::try_from(len).ok().filter(|_| len <= MAX_TEXT).ok_or(EncodeError::TooLong(len))?;
    let mut out = Vec::with_capacity(7 + len);
    out.push(msg.flags);
    out.extend_from_slice(&wire_len.to_be_bytes());
    if let Some(reply_to) = msg.reply_to {
        out.extend_from_slice(&reply_to.to_be_bytes());
    }
    out.extend_from_slice(msg.text.as_bytes());
    Ok(out)
}

pub fn decode(buf: &[u8]) -> Result<(Message, usize), DecodeError> {
    let Some(&flags) = buf.first() else {
        return Err(DecodeError::Incomplete { needed: 3 });
    };
    if flags & !KNOWN != 0 {
        return Err(DecodeError::UnknownFlags(flags));
    }
    let Some(len_bytes) = buf.get(1..3) else {
        return Err(DecodeError::Incomplete { needed: 3 - buf.len() });
    };
    let len = usize::from(u16::from_be_bytes([len_bytes[0], len_bytes[1]]));
    if len > MAX_TEXT {
        return Err(DecodeError::TooLong(len));
    }
    let head = if flags & REPLY != 0 { 7 } else { 3 };
    let total = head + len;
    if buf.len() < total {
        return Err(DecodeError::Incomplete { needed: total - buf.len() });
    }
    let reply_to = (head == 7).then(|| u32::from_be_bytes([buf[3], buf[4], buf[5], buf[6]]));
    let text = std::str::from_utf8(&buf[head..total]).map_err(|_| DecodeError::BadUtf8)?;
    Ok((Message { flags, reply_to, text: text.to_string() }, total))
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
        self.buf.extend_from_slice(chunk);
    }

    pub fn next_message(&mut self) -> Result<Option<Message>, DecodeError> {
        match decode(&self.buf) {
            Ok((msg, used)) => {
                self.buf.drain(..used);
                Ok(Some(msg))
            }
            Err(DecodeError::Incomplete { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn buffered(&self) -> usize {
        self.buf.len()
    }
}

pub fn hexdump(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (line, chunk) in bytes.chunks(16).enumerate() {
        write!(out, "{:08x}  ", line * 16).expect("запись в String не может провалиться");
        for i in 0..16 {
            match chunk.get(i) {
                Some(b) => write!(out, "{b:02x} ").expect("запись в String не может провалиться"),
                None => out.push_str("   "),
            }
        }
        out.push('|');
        out.extend(chunk.iter().map(|b| if b.is_ascii_graphic() || *b == b' ' { char::from(*b) } else { '.' }));
        out.push_str("|\n");
    }
    out
}

pub fn preview(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    if max_bytes < '…'.len_utf8() {
        return String::new();
    }
    let mut cut = max_bytes - '…'.len_utf8();
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…", &text[..cut])
}
