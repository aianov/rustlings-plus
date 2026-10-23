use crate::domain::{PostError, PostService};
use contracts::api::{Post, PostApi};
use contracts::errors::{ApiError, Code, GrpcStatus, grpc_status};

pub struct PostController {
    service: PostService,
}

impl PostController {
    pub fn new(service: PostService) -> PostController {
        PostController { service }
    }
}

pub fn to_api(err: &PostError) -> ApiError {
    todo!()
}

impl PostApi for PostController {
    fn create(&self, author_id: u64, text: &str, request_id: &str) -> Result<Post, GrpcStatus> {
        self.service.create(author_id, text, request_id).map_err(|err| grpc_status(&to_api(&err), request_id))
    }
}
