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
        match self {
            Code::Malformed => "request.malformed",
            Code::ValidationFailed => "validation.failed",
            Code::Unauthenticated => "auth.unauthenticated",
            Code::Forbidden => "auth.forbidden",
            Code::UserNotFound => "user.not_found",
            Code::UsernameTaken => "user.username_taken",
            Code::PostNotFound => "post.not_found",
            Code::AuthorNotFound => "post.author_not_found",
            Code::RateLimited => "rate_limited",
            Code::ServiceUnavailable => "service.unavailable",
            Code::Timeout => "service.timeout",
            Code::Internal => "internal",
        }
    }

    pub fn from_wire(text: &str) -> Option<Code> {
        Code::ALL.into_iter().find(|code| code.as_str() == text)
    }

    pub fn http_status(self) -> u16 {
        match self {
            Code::Malformed => 400,
            Code::ValidationFailed | Code::AuthorNotFound => 422,
            Code::Unauthenticated => 401,
            Code::Forbidden => 403,
            Code::UserNotFound | Code::PostNotFound => 404,
            Code::UsernameTaken => 409,
            Code::RateLimited => 429,
            Code::ServiceUnavailable => 503,
            Code::Timeout => 504,
            Code::Internal => 500,
        }
    }

    pub fn grpc(self) -> GrpcCode {
        match self {
            Code::Malformed | Code::ValidationFailed | Code::AuthorNotFound => GrpcCode::InvalidArgument,
            Code::Unauthenticated => GrpcCode::Unauthenticated,
            Code::Forbidden => GrpcCode::PermissionDenied,
            Code::UserNotFound | Code::PostNotFound => GrpcCode::NotFound,
            Code::UsernameTaken => GrpcCode::AlreadyExists,
            Code::RateLimited => GrpcCode::ResourceExhausted,
            Code::ServiceUnavailable => GrpcCode::Unavailable,
            Code::Timeout => GrpcCode::DeadlineExceeded,
            Code::Internal => GrpcCode::Internal,
        }
    }

    pub fn public_message(self) -> &'static str {
        match self {
            Code::Malformed => "запрос нельзя прочитать",
            Code::ValidationFailed => "проверь поля",
            Code::Unauthenticated => "нужно войти",
            Code::Forbidden => "нет прав",
            Code::UserNotFound => "пользователь не найден",
            Code::UsernameTaken => "имя занято",
            Code::PostNotFound => "пост не найден",
            Code::AuthorNotFound => "автор не найден",
            Code::RateLimited => "слишком много запросов",
            Code::ServiceUnavailable => "сервис временно недоступен",
            Code::Timeout => "сервис не успел ответить",
            Code::Internal => "внутренняя ошибка",
        }
    }

    pub fn is_client_fault(self) -> bool {
        self.http_status() < 500
    }
}
