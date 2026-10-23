use std::sync::Arc;

use contracts::{PostId, Role};
use grpc_kit::role_of;
use proto::invalid;
use proto::social::v1::{Changed, CreatePostRequest, EditPostRequest, LikeReply, Post, PostAction};
use tonic::{Request, Response, Status};

use super::{PostsController, finish, origin};
use crate::common::Actor;
use crate::dto;
use crate::service::{MAX_KEY, good_key};

impl PostsController {
    pub(super) async fn create(
        &self,
        request: Request<CreatePostRequest>,
    ) -> Result<Response<Post>, Status> {
        let (author, users, rid) = self.actor(&request)?;
        let draft = dto::draft(request.get_ref()).map_err(|errors| invalid(&errors))?;
        let draft = self.previewed(draft).await;
        let key = request.get_ref().idempotency_key.clone();
        if !key.is_empty() && !good_key(&key) {
            return Err(Status::invalid_argument(format!(
                "ключ идемпотентности: 1..={MAX_KEY} печатных ASCII-символов"
            )));
        }
        let origin = origin(&request);
        let service = Arc::clone(&self.service);
        let created = async move {
            service
                .create_once(&users, author, Some(key.as_str()), draft, &origin)
                .await
        };
        let post = finish(created, &rid).await?;
        Ok(Response::new(dto::post(&post)))
    }

    pub(super) async fn edit(
        &self,
        request: Request<EditPostRequest>,
    ) -> Result<Response<Post>, Status> {
        let (user, _, rid) = self.actor(&request)?;
        let origin = origin(&request);
        let actor = Actor {
            id: user,
            role: Role::parse(role_of(&request)),
        };
        let text = dto::edit_text(request.get_ref()).map_err(|errors| invalid(&errors))?;
        let post = PostId(request.into_inner().post_id);
        let service = Arc::clone(&self.service);
        let editing = async move { service.edit(actor, post, text, &origin).await };
        let edited = finish(editing, &rid).await?;
        Ok(Response::new(dto::post(&edited)))
    }

    pub(super) async fn delete(
        &self,
        request: Request<PostAction>,
    ) -> Result<Response<Changed>, Status> {
        let (user, _, rid) = self.actor(&request)?;
        let origin = origin(&request);
        let actor = Actor {
            id: user,
            role: Role::parse(role_of(&request)),
        };
        let post = PostId(request.into_inner().post_id);
        let service = Arc::clone(&self.service);
        let deleting = async move { service.delete(actor, post, &origin).await };
        let changed = finish(deleting, &rid).await?;
        Ok(Response::new(Changed { changed }))
    }

    pub(super) async fn like_post(
        &self,
        request: Request<PostAction>,
    ) -> Result<Response<LikeReply>, Status> {
        let (user, _, rid) = self.actor(&request)?;
        let origin = origin(&request);
        let post = PostId(request.into_inner().post_id);
        let service = Arc::clone(&self.service);
        let liking = async move { service.like(user, post, &origin).await };
        let liked = finish(liking, &rid).await?;
        Ok(Response::new(LikeReply {
            changed: liked.changed,
            author_id: liked.author.0,
        }))
    }
}
