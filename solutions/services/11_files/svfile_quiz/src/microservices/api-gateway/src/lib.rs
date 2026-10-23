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
        let body = limit(body, files::image::MAX_BYTES as u64, declared_len).map_err(|_| AvatarError::TooLarge)?;
        self.users.set_avatar(user, body, now).await?;
        self.users.avatar_link(user, now).ok_or(AvatarError::Db)
    }

    pub async fn download(&self, path_and_query: &str, now: u64) -> Response {
        let opened = match self.users.open(path_and_query, now) {
            Ok(opened) => opened,
            Err(LinkError::Malformed) => return Response::empty(400),
            Err(LinkError::BadSignature) => return Response::empty(403),
            Err(LinkError::Expired) => return Response::empty(410),
        };
        match self.users.read(&opened.key).await {
            Ok(Some(bytes)) => serve(&Meta { client_name: opened.name }, &bytes),
            Ok(None) => Response::empty(404),
            Err(_) => Response::empty(500),
        }
    }
}
