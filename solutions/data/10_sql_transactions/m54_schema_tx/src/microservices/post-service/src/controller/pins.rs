use contracts::{CommentId, PostId, UserId};
use proto::social::v1::{
    AuthorPostsRequest, Changed, CommentAction, CommentLikeReply, FeedReply, PostAction,
};
use tonic::{Request, Response, Status};

use super::{PostsController, failed};
use crate::dto;

impl PostsController {
    pub(super) async fn pin_post(
        &self,
        request: Request<PostAction>,
        on: bool,
    ) -> Result<Response<Changed>, Status> {
        let (author, _, rid) = self.actor(&request)?;
        let post = PostId(request.into_inner().post_id);
        let changed = self.service.pin(author, post, on).await;
        let changed = changed.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(Changed { changed }))
    }

    pub(super) async fn pinned_of(
        &self,
        request: Request<AuthorPostsRequest>,
    ) -> Result<Response<FeedReply>, Status> {
        let rid = proto::request_id(request.metadata());
        let author = UserId(request.into_inner().author_id);
        let posts = self.service.pinned(author).await;
        let posts = posts.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(FeedReply {
            posts: posts.iter().map(dto::post).collect(),
            next: String::new(),
        }))
    }

    pub(super) async fn like_comment_of(
        &self,
        request: Request<CommentAction>,
    ) -> Result<Response<CommentLikeReply>, Status> {
        let (user, _, rid) = self.actor(&request)?;
        let comment = CommentId(request.into_inner().comment_id);
        let liked = self.service.like_comment(user, comment).await;
        let (changed, likes) = liked.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(CommentLikeReply { changed, likes }))
    }
}
