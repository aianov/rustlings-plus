
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrpcCode {
    InvalidArgument,
    NotFound,
    PermissionDenied,
    Unauthenticated,
    FailedPrecondition,
    Internal,
    Unavailable,
    DeadlineExceeded,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub code: GrpcCode,
    pub message: String,
    pub app_code: Option<String>,
}

impl Status {
    pub fn new(code: GrpcCode, message: &str) -> Status {
        Status { code, message: message.to_string(), app_code: None }
    }

    pub fn with_app_code(mut self, app_code: &str) -> Status {
        self.app_code = Some(app_code.to_string());
        self
    }
}

#[derive(Debug, PartialEq)]
pub enum PostError {
    PostNotFound(u64),
    AuthorNotFound(u64),
    UserServiceUnavailable,
    UserServiceTimeout,
    Internal(String),
}

pub const PUBLIC_CODES: [(&str, GrpcCode); 5] = [
    ("post.not_found", GrpcCode::NotFound),
    ("post.author_not_found", GrpcCode::InvalidArgument),
    ("service.unavailable", GrpcCode::Unavailable),
    ("service.timeout", GrpcCode::DeadlineExceeded),
    ("internal", GrpcCode::Internal),
];

pub fn from_user_service(author_id: u64, status: &Status) -> PostError {
    match status.code {
        GrpcCode::NotFound => PostError::AuthorNotFound(author_id),
        GrpcCode::Unavailable => PostError::UserServiceUnavailable,
        GrpcCode::DeadlineExceeded => PostError::UserServiceTimeout,
        other => PostError::Internal(format!("user-service ответил {other:?}: {}", status.message)),
    }
}

impl PostError {
    pub fn to_status(&self) -> Status {
        match self {
            PostError::PostNotFound(id) => Status::new(GrpcCode::NotFound, &format!("пост {id} не найден")).with_app_code("post.not_found"),
            PostError::AuthorNotFound(id) => {
                Status::new(GrpcCode::InvalidArgument, &format!("автор {id} не найден")).with_app_code("post.author_not_found")
            }
            PostError::UserServiceUnavailable => {
                Status::new(GrpcCode::Unavailable, "сервис временно недоступен").with_app_code("service.unavailable")
            }
            PostError::UserServiceTimeout => {
                Status::new(GrpcCode::DeadlineExceeded, "сервис не успел ответить").with_app_code("service.timeout")
            }
            PostError::Internal(_) => Status::new(GrpcCode::Internal, "внутренняя ошибка").with_app_code("internal"),
        }
    }
}

pub fn gateway_http(status: &Status) -> (u16, String) {
    let (http, default_code) = match status.code {
        GrpcCode::InvalidArgument => (422, "validation.failed"),
        GrpcCode::NotFound => (404, "not_found"),
        GrpcCode::PermissionDenied | GrpcCode::Unauthenticated => (500, "internal"),
        GrpcCode::Unavailable => (503, "service.unavailable"),
        GrpcCode::DeadlineExceeded => (504, "service.timeout"),
        GrpcCode::FailedPrecondition | GrpcCode::Internal => (500, "internal"),
    };
    let code = status.app_code.as_deref().filter(|c| PUBLIC_CODES.contains(&(*c, status.code))).unwrap_or(default_code);
    (http, code.to_string())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn through_the_chain(user_service_answer: Status) -> (u16, String) {
        let post_error = from_user_service(77, &user_service_answer);
        gateway_http(&post_error.to_status())
    }

    #[test]
    fn missing_author_is_a_request_problem_not_a_missing_post() {
        let (http, code) = through_the_chain(Status::new(GrpcCode::NotFound, "user 77 not found"));
        assert_eq!((http, code.as_str()), (422, "post.author_not_found"), "поста тут нет и не было: клиент указал неверного автора");
        assert_eq!(from_user_service(77, &Status::new(GrpcCode::NotFound, "x")), PostError::AuthorNotFound(77), "в ошибке остаётся id автора");
    }

    #[test]
    fn neighbours_unavailability_is_ours_to_report_as_temporary() {
        let (http, code) = through_the_chain(Status::new(GrpcCode::Unavailable, "connection refused"));
        assert_eq!((http, code.as_str()), (503, "service.unavailable"), "сосед недоступен: клиенту 503");
        let (http, code) = through_the_chain(Status::new(GrpcCode::DeadlineExceeded, "deadline"));
        assert_eq!((http, code.as_str()), (504, "service.timeout"), "сосед не успел: 504");
    }

    #[test]
    fn our_own_misconfiguration_is_not_the_clients_fault() {
        for code in [GrpcCode::PermissionDenied, GrpcCode::Unauthenticated, GrpcCode::InvalidArgument, GrpcCode::FailedPrecondition] {
            let (http, name) = through_the_chain(Status::new(code, "service token rejected"));
            assert_eq!((http, name.as_str()), (500, "internal"), "{code:?} от соседа при вызове сервис-к-сервису значит, что сломали мы: клиент ни при чём");
        }
    }

    #[test]
    fn neighbour_text_stays_in_the_logs() {
        let post_error = from_user_service(1, &Status::new(GrpcCode::PermissionDenied, "token for post-service expired"));
        match &post_error {
            PostError::Internal(text) => assert!(text.contains("token for post-service expired"), "для логов причина сохраняется: {text}"),
            other => panic!("ожидали Internal, пришло {other:?}"),
        }
        let status = post_error.to_status();
        assert!(!status.message.contains("token"), "наружу из сервиса текст соседа не уходит: {}", status.message);
        assert_eq!(status.code, GrpcCode::Internal, "внутренняя ошибка");
    }

    #[test]
    fn own_errors_keep_their_meaning() {
        let (http, code) = gateway_http(&PostError::PostNotFound(5).to_status());
        assert_eq!((http, code.as_str()), (404, "post.not_found"), "свой пост не найден это честные 404");
        assert_eq!(PostError::PostNotFound(5).to_status().code, GrpcCode::NotFound, "а на уровне gRPC NotFound");
    }

    #[test]
    fn gateway_does_not_trust_codes_it_does_not_know() {
        let status = Status::new(GrpcCode::Unavailable, "x").with_app_code("internal.pg_pool_exhausted");
        let (http, code) = gateway_http(&status);
        assert_eq!((http, code.as_str()), (503, "service.unavailable"), "неизвестный код соседа заменяется кодом по типу gRPC");
        let status = Status::new(GrpcCode::NotFound, "x");
        assert_eq!(gateway_http(&status), (404, "not_found".to_string()), "без прикладного кода работает код по умолчанию");
        let lying = Status::new(GrpcCode::Internal, "x").with_app_code("post.not_found");
        assert_eq!(gateway_http(&lying), (500, "internal".to_string()), "код из каталога, который не сходится со статусом (Internal и post.not_found), не принимается: верим gRPC-коду");
        assert_eq!(gateway_http(&Status::new(GrpcCode::PermissionDenied, "x")).0, 500, "отказ в правах от соседа не превращается в 403 для клиента");
    }
}
