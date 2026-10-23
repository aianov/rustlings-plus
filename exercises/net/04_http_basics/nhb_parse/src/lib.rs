use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Head,
    Post,
    Put,
    Delete,
    Patch,
    Options,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    Http10,
    Http11,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    BadRequestLine,
    BadMethod,
    BadVersion,
    BadTarget,
    BadEscape,
    BadHeader,
    BadContentLength,
    Smuggling,
    MissingHost,
    DuplicateHost,
    HeadTooLarge,
    TooManyHeaders,
    BodyTooLarge,
    BadChunk,
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_head_bytes: usize,
    pub max_headers: usize,
    pub max_body: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { max_head_bytes: 8192, max_headers: 64, max_body: 1 << 20 }
    }
}

impl Method {
    pub fn as_str(self) -> &'static str {
        todo!()
    }
}

impl FromStr for Method {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Method, ParseError> {
        todo!()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Headers {
    items: Vec<(String, String)>,
}

impl Headers {
    pub fn new() -> Headers {
        Headers::default()
    }

    pub fn append(&mut self, name: &str, value: &str) {
        todo!()
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        todo!()
    }

    pub fn get_all(&self, name: &str) -> Vec<&str> {
        todo!()
    }

    pub fn remove(&mut self, name: &str) -> usize {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        std::iter::empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: Method,
    pub target: String,
    pub version: Version,
    pub headers: Headers,
    pub body: Vec<u8>,
}

impl Request {
    pub fn path(&self) -> Result<String, ParseError> {
        todo!()
    }

    pub fn query(&self) -> Result<Vec<(String, String)>, ParseError> {
        todo!()
    }

    pub fn keep_alive(&self) -> bool {
        todo!()
    }
}

#[derive(Debug, PartialEq)]
pub enum Parsed {
    Partial,
    Complete { request: Request, consumed: usize },
}

pub fn parse_request(buf: &[u8], limits: &Limits) -> Result<Parsed, ParseError> {
    todo!()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusCode(u16);

impl StatusCode {
    pub fn new(code: u16) -> Option<StatusCode> {
        todo!()
    }

    pub fn code(self) -> u16 {
        todo!()
    }

    pub fn reason(self) -> &'static str {
        todo!()
    }

    pub fn allows_body(self) -> bool {
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    pub status: StatusCode,
    pub headers: Headers,
    pub body: Vec<u8>,
}

impl Response {
    pub fn new(status: StatusCode) -> Response {
        Response { status, headers: Headers::new(), body: Vec::new() }
    }

    pub fn header(&mut self, name: &str, value: &str) -> Result<(), ParseError> {
        todo!()
    }

    pub fn to_bytes(&self, head_only: bool) -> Vec<u8> {
        todo!()
    }
}
