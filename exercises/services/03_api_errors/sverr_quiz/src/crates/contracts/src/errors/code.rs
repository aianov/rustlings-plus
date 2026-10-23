use super::grpc::GrpcCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    Malformed,
    ValidationFailed,
    Unauthenticated,
    Forbidden,
    UserNotFound,
    UsernameTaken,
    PostNotFound,
    AuthorNotFound,
    RateLimited,
    ServiceUnavailable,
    Timeout,
    Internal,
}

impl Code {
    pub const ALL: [Code; 12] = [
        Code::Malformed,
        Code::ValidationFailed,
        Code::Unauthenticated,
        Code::Forbidden,
        Code::UserNotFound,
        Code::UsernameTaken,
        Code::PostNotFound,
        Code::AuthorNotFound,
        Code::RateLimited,
        Code::ServiceUnavailable,
        Code::Timeout,
        Code::Internal,
    ];

    pub fn as_str(self) -> &'static str {
        todo!()
    }

    pub fn from_wire(text: &str) -> Option<Code> {
        todo!()
    }

    pub fn http_status(self) -> u16 {
        todo!()
    }

    pub fn grpc(self) -> GrpcCode {
        todo!()
    }

    pub fn public_message(self) -> &'static str {
        todo!()
    }

    pub fn is_client_fault(self) -> bool {
        self.http_status() < 500
    }
}
