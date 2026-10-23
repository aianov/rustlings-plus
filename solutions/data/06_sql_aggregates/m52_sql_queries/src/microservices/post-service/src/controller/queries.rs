use contracts::{PostId, Role, UserId};
use grpc_kit::{caller_of, role_of};
use proto::request_id;
use proto::social::v1::{
    AuthorPostsRequest, AuthorTotals, DailyReply, DailyRequest, Day, FeedReply, Post as PostReply,
    PostsByIds,
};
use tonic::{Request, Response, Status};

use super::{PostsController, failed};
use crate::common::Post;
use crate::dto;

impl PostsController {
    pub(super) async fn marked(&self, viewer: Option<UserId>, posts: &[Post]) -> Vec<PostReply> {
        let liked = self.service.liked_by(viewer, posts).await;
        posts
            .iter()
            .map(|post| dto::marked(post, liked.contains(&post.id)))
            .collect()
    }

    pub(super) async fn hydrate_posts(
        &self,
        request: Request<PostsByIds>,
    ) -> Result<Response<FeedReply>, Status> {
        let viewer = caller_of(&request).ok().map(UserId);
        let rid = request_id(request.metadata());
        let ask = request.into_inner();
        let ids = ask.ids.into_iter().map(PostId).collect();
        let hidden = ask.hidden_authors.into_iter().map(UserId).collect();
        let found = self.service.hydrate(viewer, ids, hidden).await;
        let found = found.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(FeedReply {
            posts: found
                .iter()
                .map(|(post, liked)| dto::marked(post, *liked))
                .collect(),
            next: String::new(),
        }))
    }

    pub(super) async fn count_author(
        &self,
        request: Request<AuthorPostsRequest>,
    ) -> Result<Response<AuthorTotals>, Status> {
        let rid = request_id(request.metadata());
        let author = UserId(request.into_inner().author_id);
        let totals = self.service.totals(author).await;
        let totals = totals.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(AuthorTotals {
            posts: totals.posts,
            likes: totals.likes,
        }))
    }

    pub(super) async fn daily_of(
        &self,
        request: Request<DailyRequest>,
    ) -> Result<Response<DailyReply>, Status> {
        let role = Role::parse(role_of(&request));
        let rid = request_id(request.metadata());
        let days = self.service.daily(role, request.into_inner().days).await;
        let days = days.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(DailyReply {
            days: days
                .into_iter()
                .map(|day| Day {
                    day: day.day,
                    posts: day.posts,
                    likes: day.likes,
                })
                .collect(),
        }))
    }
}
