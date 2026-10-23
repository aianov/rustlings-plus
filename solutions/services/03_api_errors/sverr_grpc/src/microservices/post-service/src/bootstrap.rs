use crate::controller::PostController;
use crate::domain::{Post, PostService, UserDirectory};
use std::sync::Arc;

pub fn build_with(directory: Arc<dyn UserDirectory>, seed: Vec<Post>) -> PostController {
    PostController::new(Arc::new(PostService::new(directory, seed)))
}
