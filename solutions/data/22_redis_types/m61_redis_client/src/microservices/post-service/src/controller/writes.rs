use std::sync::Arc;

use contracts::{CommentId, PostId, Role};
use grpc_kit::role_of;
use proto::invalid;
use proto::social::v1::{
    Changed, CommentReply, CommentRequest, CreatePostRequest, EditPostRequest, LikeReply, Post,
    PostAction,
};
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
        self.service.tagged(&post).await;
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

    pub(super) async fn comment_on(
        &self,
        request: Request<CommentRequest>,
    ) -> Result<Response<CommentReply>, Status> {
        let (author, _, rid) = self.actor(&request)?;
        let text = dto::comment_text(request.get_ref()).map_err(|errors| invalid(&errors))?;
        let origin = origin(&request);
        let service = Arc::clone(&self.service);
        let ask = request.into_inner();
        let post = PostId(ask.post_id);
        let parent = CommentId(ask.parent_id);
        let commented = async move {
            match parent.0 {
                0 => service.comment(author, post, text, &origin).await,
                _ => service.reply_to(author, post, parent, text, &origin).await,
            }
        };
        let comment = finish(commented, &rid).await?;
        Ok(Response::new(CommentReply {
            comment_id: comment.0,
        }))
    }
}
