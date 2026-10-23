use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StatusCode(u16);

impl StatusCode {
    pub const OK: StatusCode = StatusCode(200);
    pub const CREATED: StatusCode = StatusCode(201);
    pub const BAD_REQUEST: StatusCode = StatusCode(400);
    pub const NOT_FOUND: StatusCode = StatusCode(404);
    pub const INTERNAL: StatusCode = StatusCode(500);

    pub fn new(code: u16) -> Option<StatusCode> {
        (100..=599).contains(&code).then_some(StatusCode(code))
    }

    pub fn code(self) -> u16 {
        self.0
    }

    pub fn class(self) -> u16 {
        self.0 / 100
    }

    pub fn is_success(self) -> bool {
        self.class() == 2
    }

    pub fn reason(self) -> &'static str {
        match self.0 {
            100 => "Continue",
            101 => "Switching Protocols",
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            206 => "Partial Content",
            301 => "Moved Permanently",
            302 => "Found",
            304 => "Not Modified",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            408 => "Request Timeout",
            409 => "Conflict",
            413 => "Payload Too Large",
            414 => "URI Too Long",
            415 => "Unsupported Media Type",
            422 => "Unprocessable Entity",
            429 => "Too Many Requests",
            431 => "Request Header Fields Too Large",
            500 => "Internal Server Error",
            501 => "Not Implemented",
            503 => "Service Unavailable",
            505 => "HTTP Version Not Supported",
            _ => "",
        }
    }

    pub fn allows_body(self) -> bool {
        self.0 >= 200 && self.0 != 204 && self.0 != 304
    }
}

impl fmt::Display for StatusCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.0, self.reason())
    }
}
