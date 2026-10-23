mod frame;
mod grpc;
mod hpack;

use std::collections::{BTreeMap, HashMap};

use frame::{Frame, read_frames};
use grpc::{Messages, parse_timeout, percent_decode};
use hpack::{Decoder, Header};
use std::time::Duration;

#[derive(Debug, PartialEq, Clone)]
pub struct GrpcStatus {
    pub code: u32,
    pub message: String,
}

#[derive(Debug, PartialEq, Clone, Default)]
pub struct Call {
    pub stream: u32,
    pub path: String,
    pub timeout: Option<Duration>,
    pub request_messages: Vec<Vec<u8>>,
    pub response_messages: Vec<Vec<u8>>,
    pub status: Option<GrpcStatus>,
    pub reset: Option<u32>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Goaway {
    pub last_stream: u32,
    pub code: u32,
    pub debug: String,
}

#[derive(Debug, PartialEq, Default)]
pub struct Report {
    pub calls: Vec<Call>,
    pub client_settings: Vec<(u16, u32)>,
    pub server_settings: Vec<(u16, u32)>,
    pub goaway: Option<Goaway>,
    pub pings: usize,
}

#[derive(Debug, PartialEq)]
pub enum DumpError {
    BadPreface,
    Incomplete,
    Frame(String),
    Hpack(String),
    Grpc(String),
    Unsupported(String),
}

const PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

fn header<'a>(headers: &'a [Header], name: &str) -> Option<&'a str> {
    headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

fn decode_block(decoder: &mut Decoder, block: &[u8], end_headers: bool) -> Result<Vec<Header>, DumpError> {
    if !end_headers {
        return Err(DumpError::Unsupported("блоки заголовков из нескольких кадров (CONTINUATION)".to_string()));
    }
    decoder.decode(block).map_err(|e| DumpError::Hpack(format!("{e:?}")))
}

fn note_goaway(report: &mut Report, last_stream: u32, code: u32, debug: Vec<u8>) {
    report.goaway = Some(Goaway { last_stream, code, debug: String::from_utf8_lossy(&debug).into_owned() });
}

pub fn analyze(client: &[u8], server: &[u8]) -> Result<Report, DumpError> {
    let client = client.strip_prefix(PREFACE).ok_or(DumpError::BadPreface)?;
    let mut report = Report::default();
    let mut calls: BTreeMap<u32, Call> = BTreeMap::new();

    let mut decoder = Decoder::new(4096);
    let mut requests: HashMap<u32, Messages> = HashMap::new();
    for frame in read_frames(client)? {
        match frame {
            Frame::Settings { ack: false, entries } => report.client_settings.extend(entries),
            Frame::Headers { stream, block, end_headers, .. } => {
                let headers = decode_block(&mut decoder, &block, end_headers)?;
                calls.entry(stream).or_insert_with(|| Call {
                    stream,
                    path: header(&headers, ":path").unwrap_or_default().to_string(),
                    timeout: header(&headers, "grpc-timeout").and_then(parse_timeout),
                    ..Call::default()
                });
            }
            Frame::Data { stream, data, .. } => {
                if let Some(call) = calls.get_mut(&stream) {
                    let messages = requests.entry(stream).or_insert_with(Messages::new).feed(&data).map_err(DumpError::Grpc)?;
                    call.request_messages.extend(messages);
                }
            }
            Frame::RstStream { stream, code } => {
                if let Some(call) = calls.get_mut(&stream) {
                    call.reset = Some(code);
                }
            }
            Frame::Ping { ack: false } => report.pings += 1,
            Frame::Goaway { last_stream, code, debug } => note_goaway(&mut report, last_stream, code, debug),
            _ => {}
        }
    }

    let mut decoder = Decoder::new(4096);
    let mut responses: HashMap<u32, Messages> = HashMap::new();
    for frame in read_frames(server)? {
        match frame {
            Frame::Settings { ack: false, entries } => report.server_settings.extend(entries),
            Frame::Headers { stream, block, end_headers, .. } => {
                let headers = decode_block(&mut decoder, &block, end_headers)?;
                if let (Some(call), Some(code)) = (calls.get_mut(&stream), header(&headers, "grpc-status")) {
                    let message = header(&headers, "grpc-message").map(percent_decode).unwrap_or_default();
                    call.status = Some(GrpcStatus { code: code.parse().unwrap_or(2), message });
                }
            }
            Frame::Data { stream, data, .. } => {
                if let Some(call) = calls.get_mut(&stream) {
                    let messages = responses.entry(stream).or_insert_with(Messages::new).feed(&data).map_err(DumpError::Grpc)?;
                    call.response_messages.extend(messages);
                }
            }
            Frame::RstStream { stream, code } => {
                if let Some(call) = calls.get_mut(&stream) {
                    call.reset = Some(code);
                }
            }
            Frame::Ping { ack: false } => report.pings += 1,
            Frame::Goaway { last_stream, code, debug } => note_goaway(&mut report, last_stream, code, debug),
            _ => {}
        }
    }
    report.calls = calls.into_values().collect();
    Ok(report)
}
