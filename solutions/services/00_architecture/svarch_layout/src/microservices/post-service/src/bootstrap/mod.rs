use crate::controller::Controller;
use crate::repository::InMemoryPosts;
use crate::service::PostService;
use std::sync::Arc;

pub fn build() -> Controller {
    let posts = Arc::new(InMemoryPosts::default());
    Controller::new(PostService::new(posts))
}
