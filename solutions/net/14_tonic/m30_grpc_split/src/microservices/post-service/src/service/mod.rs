use proto::request_id;
use proto::social::v1::posts_server::Posts;
use proto::social::v1::{
    Changed, CommentReply, CommentRequest, CreatePostRequest, FeedReply, FeedRequest,
    GetPostRequest, LikeReply, Post, PostAction, Trend, TrendsReply, TrendsRequest,
};
use social_core::{App, DomainError, MAX_TRENDS, PostId, PostItem, UserId, Visibility};
use tonic::{Request, Response, Status};

use crate::mirror::{Mirror, failed};

pub struct PostsService {
    app: App,
    mirror: Mirror,
}

fn post(item: &PostItem) -> Post {
    Post {
        id: item.id.0,
        author_id: item.author.0,
        author_name: item.author_name.to_string(),
        text: item.text.to_string(),
        likes: item.likes as u64,
    }
}

impl PostsService {
    pub fn new(app: App, mirror: Mirror) -> PostsService {
        PostsService { app, mirror }
    }

    async fn item(&self, id: PostId, rid: &str) -> Result<PostItem, Status> {
        match self.app.post(id).await {
            Some(item) => Ok(item),
            None => Err(failed(DomainError::UnknownPost(id), rid)),
        }
    }
}

#[tonic::async_trait]
impl Posts for PostsService {
    async fn create_post(
        &self,
        request: Request<CreatePostRequest>,
    ) -> Result<Response<Post>, Status> {
        let rid = request_id(request.metadata());
        let ask = request.into_inner();
        let author = UserId(ask.author_id);
        self.mirror.profile(author, &rid).await?;
        self.mirror.ensure(&self.app, author, &rid).await?;
        let published = self
            .app
            .publish(author, &ask.text, Visibility::Public)
            .await;
        let id = published.map_err(|error| failed(error, &rid))?;
        let item = self.item(id, &rid).await?;
        Ok(Response::new(post(&item)))
    }

    async fn get_post(&self, request: Request<GetPostRequest>) -> Result<Response<Post>, Status> {
        let rid = request_id(request.metadata());
        let id = PostId(request.into_inner().post_id);
        match self.app.public_post(id).await {
            Some(item) => Ok(Response::new(post(&item))),
            None => Err(failed(DomainError::UnknownPost(id), &rid)),
        }
    }

    async fn like(&self, request: Request<PostAction>) -> Result<Response<LikeReply>, Status> {
        let rid = request_id(request.metadata());
        let ask = request.into_inner();
        let (user, id) = (UserId(ask.user_id), PostId(ask.post_id));
        self.mirror.ensure(&self.app, user, &rid).await?;
        let liked = self.app.like(user, id).await;
        let changed = liked.map_err(|error| failed(error, &rid))?;
        let author = self.item(id, &rid).await?.author;
        Ok(Response::new(LikeReply {
            changed,
            author_id: author.0,
        }))
    }

    async fn delete_post(&self, request: Request<PostAction>) -> Result<Response<Changed>, Status> {
        let rid = request_id(request.metadata());
        let ask = request.into_inner();
        let (user, id) = (UserId(ask.user_id), PostId(ask.post_id));
        self.mirror.ensure(&self.app, user, &rid).await?;
        let deleted = self.app.delete_post(user, id).await;
        let changed = deleted.map_err(|error| failed(error, &rid))?;
        Ok(Response::new(Changed { changed }))
    }

    async fn feed(&self, request: Request<FeedRequest>) -> Result<Response<FeedReply>, Status> {
        let rid = request_id(request.metadata());
        let ask = request.into_inner();
        let viewer = UserId(ask.viewer_id);
        self.mirror.ensure(&self.app, viewer, &rid).await?;
        self.mirror.sync_viewer(&self.app, viewer, &rid).await?;
        let size = ask.size as usize;
        let skip = (ask.page as usize)
            .checked_sub(1)
            .and_then(|before| before.checked_mul(size));
        let items = match skip {
            Some(skip) if size > 0 => self.app.feed(viewer, skip, size).await,
            _ => Vec::new(),
        };
        let posts = items.iter().map(post).collect();
        Ok(Response::new(FeedReply { posts }))
    }

    async fn comment(
        &self,
        request: Request<CommentRequest>,
    ) -> Result<Response<CommentReply>, Status> {
        let rid = request_id(request.metadata());
        let ask = request.into_inner();
        let author = UserId(ask.author_id);
        self.mirror.ensure(&self.app, author, &rid).await?;
        let comment = self
            .app
            .comment(author, PostId(ask.post_id), &ask.text)
            .await;
        let comment = comment.map_err(|error| failed(error, &rid))?;
        Ok(Response::new(CommentReply {
            comment_id: comment.0,
        }))
    }

    async fn trends(
        &self,
        request: Request<TrendsRequest>,
    ) -> Result<Response<TrendsReply>, Status> {
        let rid = request_id(request.metadata());
        let limit = match request.into_inner().limit {
            0 => MAX_TRENDS,
            limit => (limit as usize).min(MAX_TRENDS),
        };
        let refreshed = self.app.refresh_trends().await;
        refreshed.map_err(|error| failed(error, &rid))?;
        let trends = self
            .app
            .trends(limit)
            .iter()
            .map(|trend| Trend {
                post_id: trend.post.0,
                likes: trend.likes as u64,
            })
            .collect();
        Ok(Response::new(TrendsReply { trends }))
    }
}
