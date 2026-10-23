use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io;

use sha1::{Digest, Sha1};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(data: &[u8]) -> String {
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let byte = |at: usize| u32::from(chunk.get(at).copied().unwrap_or(0));
        let n = (byte(0) << 16) | (byte(1) << 8) | byte(2);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn accept_for(key: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(GUID.as_bytes());
    base64(&hasher.finalize())
}

fn broken(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("WebSocket: {what}"))
}

pub struct Ws {
    stream: TcpStream,
    buf: Vec<u8>,
    seed: u64,
    message: Vec<u8>,
    close_code: Option<u16>,
    closed: bool,
}

impl Ws {
    pub async fn connect(addr: &str, path: &str) -> io::Result<Ws> {
        let stream = TcpStream::connect(addr).await?;
        stream.set_nodelay(true)?;
        let mut ws = Ws {
            stream,
            buf: Vec::new(),
            seed: RandomState::new().build_hasher().finish() | 1,
            message: Vec::new(),
            close_code: None,
            closed: false,
        };
        let mut raw = Vec::with_capacity(16);
        raw.extend_from_slice(&ws.next_mask());
        raw.extend_from_slice(&ws.next_mask());
        raw.extend_from_slice(&ws.next_mask());
        raw.extend_from_slice(&ws.next_mask());
        let key = base64(&raw);
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        ws.stream.write_all(request.as_bytes()).await?;
        let end = loop {
            if let Some(end) = ws.buf.windows(4).position(|window| window == b"\r\n\r\n") {
                break end;
            }
            ws.fill().await?;
        };
        let head = String::from_utf8_lossy(&ws.buf[..end]).into_owned();
        ws.buf.drain(..end + 4);
        let status = head.split(' ').nth(1).unwrap_or_default();
        if status != "101" {
            let first = head.lines().next().unwrap_or_default();
            return Err(broken(&format!("шлюз не открыл WebSocket: {first}")));
        }
        let accept = head.split("\r\n").skip(1).find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("sec-websocket-accept")
                .then(|| value.trim().to_string())
        });
        if accept.as_deref() != Some(accept_for(&key).as_str()) {
            return Err(broken("Sec-WebSocket-Accept не тот"));
        }
        Ok(ws)
    }

    pub fn close_code(&self) -> Option<u16> {
        self.close_code
    }

    fn next_mask(&mut self) -> [u8; 4] {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        let mixed = self.seed.wrapping_mul(0x2545_f491_4f6c_dd1d);
        ((mixed >> 32) as u32).to_be_bytes()
    }

    async fn fill(&mut self) -> io::Result<()> {
        let mut chunk = [0u8; 8192];
        let n = self.stream.read(&mut chunk).await?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.buf.extend_from_slice(&chunk[..n]);
        Ok(())
    }

    async fn frame(&mut self, opcode: u8, payload: &[u8]) -> io::Result<()> {
        let mut out = vec![0x80 | opcode];
        let len = payload.len();
        if len < 126 {
            out.push(0x80 | len as u8);
        } else if len < 65536 {
            out.push(0x80 | 126);
            out.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            out.push(0x80 | 127);
            out.extend_from_slice(&(len as u64).to_be_bytes());
        }
        let key = self.next_mask();
        out.extend_from_slice(&key);
        out.extend(
            payload
                .iter()
                .enumerate()
                .map(|(at, byte)| byte ^ key[at % 4]),
        );
        self.stream.write_all(&out).await?;
        self.stream.flush().await
    }

    pub async fn send_text(&mut self, text: &str) -> io::Result<()> {
        if self.closed {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.frame(0x1, text.as_bytes()).await
    }

    pub async fn send_binary(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.closed {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.frame(0x2, bytes).await
    }

    pub async fn close(&mut self, code: u16) -> io::Result<()> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        self.frame(0x8, &code.to_be_bytes()).await
    }

    fn parse(&mut self) -> io::Result<Option<(bool, u8, Vec<u8>)>> {
        if self.buf.len() < 2 {
            return Ok(None);
        }
        let (b0, b1) = (self.buf[0], self.buf[1]);
        if b1 & 0x80 != 0 {
            return Err(broken("шлюз прислал замаскированный фрейм"));
        }
        let (len, header) = match b1 & 0x7f {
            126 if self.buf.len() >= 4 => (
                usize::from(u16::from_be_bytes([self.buf[2], self.buf[3]])),
                4,
            ),
            127 if self.buf.len() >= 10 => {
                let mut raw = [0u8; 8];
                raw.copy_from_slice(&self.buf[2..10]);
                (
                    usize::try_from(u64::from_be_bytes(raw))
                        .map_err(|_| broken("огромный фрейм"))?,
                    10,
                )
            }
            126 | 127 => return Ok(None),
            n => (usize::from(n), 2),
        };
        if self.buf.len() < header + len {
            return Ok(None);
        }
        let payload = self.buf[header..header + len].to_vec();
        self.buf.drain(..header + len);
        Ok(Some((b0 & 0x80 != 0, b0 & 0x0f, payload)))
    }

    pub async fn recv(&mut self) -> io::Result<Option<String>> {
        loop {
            let Some((fin, opcode, payload)) = self.parse()? else {
                if self.close_code.is_some() {
                    return Ok(None);
                }
                match self.fill().await {
                    Ok(()) => continue,
                    Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                        self.closed = true;
                        return Ok(None);
                    }
                    Err(error) => return Err(error),
                }
            };
            match opcode {
                0x0..=0x2 => {
                    self.message.extend_from_slice(&payload);
                    if fin {
                        let message = std::mem::take(&mut self.message);
                        return Ok(Some(String::from_utf8_lossy(&message).into_owned()));
                    }
                }
                0x8 => {
                    let code = match payload.get(..2) {
                        Some(raw) => u16::from_be_bytes([raw[0], raw[1]]),
                        None => 1005,
                    };
                    self.close_code = Some(code);
                    if !self.closed {
                        self.closed = true;
                        let _ = self.frame(0x8, &code.to_be_bytes()).await;
                    }
                    return Ok(None);
                }
                0x9 => self.frame(0xA, &payload).await?,
                _ => {}
            }
        }
    }
}
