use files::limit::limit;
use files::serve::{Meta, Response, serve};
use files::sign::LinkError;
use files::{Body, FileStore};
use std::sync::Arc;
use user_service::{AvatarError, AvatarService};

pub struct Gateway<S> {
    users: Arc<AvatarService<S>>,
}

impl<S: FileStore> Gateway<S> {
    pub fn new(users: Arc<AvatarService<S>>) -> Gateway<S> {
        Gateway { users }
    }

    pub async fn put_avatar(&self, user: u64, declared_len: Option<u64>, body: Body, now: u64) -> Result<String, AvatarError> {
        let _ = (user, declared_len, body, now, &self.users);
        todo!()
    }

    pub async fn download(&self, path_and_query: &str, now: u64) -> Response {
        let _ = (path_and_query, now, &self.users);
        todo!()
    }
}
