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

pub fn analyze(client: &[u8], server: &[u8]) -> Result<Report, DumpError> {
    let _ = (client, server);
    todo!()
}
