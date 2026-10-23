use crate::domain::{PostError, PostService};
use contracts::api::{Post, PostApi};
use contracts::errors::{ApiError, Code, GrpcCode, GrpcStatus, from_status, grpc_status};

pub struct PostController {
    service: PostService,
}

impl PostController {
    pub fn new(service: PostService) -> PostController {
        PostController { service }
    }
}

pub fn to_api(err: &PostError) -> ApiError {
    match err {
        PostError::TextTooLong { .. } => ApiError::new(Code::ValidationFailed).with_detail("text", "too_long"),
        PostError::UserService { author_id, status } => match status.code {
            GrpcCode::NotFound => ApiError::new(Code::AuthorNotFound).with_message(&format!("автор {author_id} не найден")),
            GrpcCode::Unavailable | GrpcCode::DeadlineExceeded | GrpcCode::ResourceExhausted => {
                let upstream = from_status(status);
                let mut err = ApiError::new(if upstream.code == Code::RateLimited { Code::ServiceUnavailable } else { upstream.code });
                err.retry_after_secs = upstream.retry_after_secs;
                err
            }
            _ => ApiError::new(Code::Internal),
        },
    }
}

impl PostApi for PostController {
    fn create(&self, author_id: u64, text: &str, request_id: &str) -> Result<Post, GrpcStatus> {
        self.service.create(author_id, text, request_id).map_err(|err| grpc_status(&to_api(&err), request_id))
    }
}
