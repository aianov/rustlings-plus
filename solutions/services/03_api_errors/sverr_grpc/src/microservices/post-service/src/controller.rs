use crate::domain::{Post, PostService};
use crate::errors::{request_id, status_from};
use crate::pb::posts_server::Posts;
use crate::pb::{CreatePostRequest, GetPostRequest, Post as PostMessage};
use std::sync::Arc;
use tonic::{Request, Response, Status};

pub struct PostController {
    service: Arc<PostService>,
}

impl PostController {
    pub fn new(service: Arc<PostService>) -> PostController {
        PostController { service }
    }
}

fn to_message(post: Post) -> PostMessage {
    PostMessage { id: post.id, author_id: post.author_id, text: post.text }
}

#[tonic::async_trait]
impl Posts for PostController {
    async fn get_post(&self, request: Request<GetPostRequest>) -> Result<Response<PostMessage>, Status> {
        let rid = request_id(&request);
        let id = request.into_inner().id;
        self.service.get(id).map(to_message).map(Response::new).map_err(|err| status_from(&err, &rid))
    }

    async fn create_post(&self, request: Request<CreatePostRequest>) -> Result<Response<PostMessage>, Status> {
        let rid = request_id(&request);
        let request = request.into_inner();
        self.service
            .create(request.author_id, &request.text)
            .map(to_message)
            .map(Response::new)
            .map_err(|err| status_from(&err, &rid))
    }
}
