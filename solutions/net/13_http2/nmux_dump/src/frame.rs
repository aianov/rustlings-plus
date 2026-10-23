use crate::DumpError;

#[derive(Debug, PartialEq)]
pub enum Frame {
    Data { stream: u32, data: Vec<u8>, end_stream: bool },
    Headers { stream: u32, block: Vec<u8>, end_stream: bool, end_headers: bool },
    RstStream { stream: u32, code: u32 },
    Settings { ack: bool, entries: Vec<(u16, u32)> },
    Ping { ack: bool },
    Goaway { last_stream: u32, code: u32, debug: Vec<u8> },
    Other,
}

const END_STREAM: u8 = 0x1;
const ACK: u8 = 0x1;
const END_HEADERS: u8 = 0x4;
const PADDED: u8 = 0x8;
const PRIORITY: u8 = 0x20;

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

fn bad(what: &str) -> DumpError {
    DumpError::Frame(what.to_string())
}

pub fn read_frames(buf: &[u8]) -> Result<Vec<Frame>, DumpError> {
    let mut frames = Vec::new();
    let mut pos = 0;
    while pos < buf.len() {
        let head = buf.get(pos..pos + 9).ok_or(DumpError::Incomplete)?;
        let length = (usize::from(head[0]) << 16) | (usize::from(head[1]) << 8) | usize::from(head[2]);
        let (kind, flags) = (head[3], head[4]);
        let stream = be32(&head[5..]) & 0x7fff_ffff;
        let body = buf.get(pos + 9..pos + 9 + length).ok_or(DumpError::Incomplete)?;
        frames.push(decode(kind, flags, stream, body)?);
        pos += 9 + length;
    }
    Ok(frames)
}

fn unpad(flags: u8, body: &[u8]) -> Result<Vec<u8>, DumpError> {
    if flags & PADDED == 0 {
        return Ok(body.to_vec());
    }
    let (&pad, rest) = body.split_first().ok_or_else(|| bad("нет байта длины выравнивания"))?;
    if usize::from(pad) >= body.len() {
        return Err(bad("выравнивание длиннее тела"));
    }
    Ok(rest[..rest.len() - usize::from(pad)].to_vec())
}

fn decode(kind: u8, flags: u8, stream: u32, body: &[u8]) -> Result<Frame, DumpError> {
    match kind {
        0 => Ok(Frame::Data { stream, data: unpad(flags, body)?, end_stream: flags & END_STREAM != 0 }),
        1 => {
            let mut block = unpad(flags, body)?;
            if flags & PRIORITY != 0 {
                if block.len() < 5 {
                    return Err(bad("короткое тело HEADERS с приоритетом"));
                }
                block.drain(..5);
            }
            Ok(Frame::Headers { stream, block, end_stream: flags & END_STREAM != 0, end_headers: flags & END_HEADERS != 0 })
        }
        3 if body.len() == 4 => Ok(Frame::RstStream { stream, code: be32(body) }),
        3 => Err(bad("RST_STREAM не из четырёх байтов")),
        4 if body.len() % 6 == 0 => Ok(Frame::Settings { ack: flags & ACK != 0, entries: body.chunks(6).map(|c| (u16::from_be_bytes([c[0], c[1]]), be32(&c[2..]))).collect() }),
        4 => Err(bad("тело SETTINGS не кратно шести")),
        6 if body.len() == 8 => Ok(Frame::Ping { ack: flags & ACK != 0 }),
        6 => Err(bad("PING не из восьми байтов")),
        7 if body.len() >= 8 => Ok(Frame::Goaway { last_stream: be32(body) & 0x7fff_ffff, code: be32(&body[4..]), debug: body[8..].to_vec() }),
        7 => Err(bad("короткий GOAWAY")),
        _ => Ok(Frame::Other),
    }
}
