use crate::controller::Controller;
use crate::repository::{InMemoryPosts, PostRepository};
use crate::service::{PostService, UserDirectory};
use std::sync::Arc;

pub fn build(users: Arc<dyn UserDirectory>) -> Controller {
    build_with(users, Arc::new(InMemoryPosts::default()))
}

pub fn build_with(users: Arc<dyn UserDirectory>, posts: Arc<dyn PostRepository>) -> Controller {
    Controller::new(PostService::new(users, posts))
}
