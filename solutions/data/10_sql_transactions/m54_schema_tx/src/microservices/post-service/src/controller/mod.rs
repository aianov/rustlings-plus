mod drafts;
mod pins;
mod queries;
mod stream;
mod threads;
mod writes;

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use contracts::{CommentId, ErrorCode, Origin, PostId, UserId};
use grpc_kit::{Stamp, caller_of};
use proto::social::v1::posts_server::Posts;
use proto::social::v1::{
    AuthorPostsRequest, AuthorTotals, Changed, CommentAction, CommentLikeReply, CommentReply,
    CommentRequest, CreatePostRequest, DailyReply, DailyRequest, DraftAction, DraftReply,
    DraftRequest, EditPostRequest, FeedEvent, FeedReply, FeedRequest, GetPostRequest, LikeReply,
    Post, PostAction, PostsByIds, ThreadReply, ThreadRequest, TopReply, TopRequest, TrendsReply,
    TrendsRequest, WatchRequest,
};
use proto::{internal, invalid, request_id, to_status};
use service_runtime::CancellationToken;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;
use tonic::{Request, Response, Status};

use crate::client::{GrpcUsers, Previewer};
use crate::dto;
use crate::service::{PostError, PostService};

pub const RECENT_MAX: u32 = 100;

pub const SYNC_BUDGET: Duration = Duration::from_secs(1);

pub struct PostsController {
    service: Arc<PostService>,
    users: Channel,
    stop: CancellationToken,
    previews: Option<Arc<Previewer>>,
}

fn failed(error: &PostError, rid: &str) -> Status {
    if error.code() == ErrorCode::Internal {
        tracing::warn!("post-service [{rid}]: {}", service_runtime::report(error));
    }
    to_status(&error.body())
}

fn origin<T>(request: &Request<T>) -> Origin {
    let stamp = Stamp::follow(request);
    Origin::new(stamp.request_id()).traced(stamp.trace_id())
}

async fn finish<T, F>(work: F, rid: &str) -> Result<T, Status>
where
    T: Send + 'static,
    F: Future<Output = Result<T, PostError>> + Send + 'static,
{
    match tokio::spawn(work).await {
        Ok(done) => done.map_err(|error| failed(&error, rid)),
        Err(..) => {
            tracing::warn!("post-service [{rid}]: задача записи упала");
            Err(internal())
        }
    }
}

impl PostsController {
    pub fn new(
        service: Arc<PostService>,
        users: Channel,
        stop: CancellationToken,
    ) -> PostsController {
        PostsController {
            service,
            users,
            stop,
            previews: None,
        }
    }

    fn actor<T>(&self, request: &Request<T>) -> Result<(UserId, GrpcUsers, String), Status> {
        let user = UserId(caller_of(request)?);
        let stamp = Stamp::follow(request);
        let rid = stamp.request_id().to_string();
        Ok((user, GrpcUsers::new(self.users.clone(), stamp), rid))
    }
}

#[tonic::async_trait]
impl Posts for PostsController {
    async fn delete_comment(
        &self,
        request: Request<CommentAction>,
    ) -> Result<Response<Changed>, Status> {
        self.remove_comment(request).await
    }

    async fn save_draft(
        &self,
        request: Request<DraftRequest>,
    ) -> Result<Response<DraftReply>, Status> {
        self.keep_draft(request).await
    }

    async fn get_draft(
        &self,
        request: Request<DraftAction>,
    ) -> Result<Response<DraftReply>, Status> {
        self.read_draft(request).await
    }

    async fn delete_draft(
        &self,
        request: Request<DraftAction>,
    ) -> Result<Response<Changed>, Status> {
        self.drop_draft(request).await
    }

    async fn create_post(
        &self,
        request: Request<CreatePostRequest>,
    ) -> Result<Response<Post>, Status> {
        self.create(request).await
    }

    async fn get_post(&self, request: Request<GetPostRequest>) -> Result<Response<Post>, Status> {
        let rid = request_id(request.metadata());
        let viewer = caller_of(&request).ok().map(UserId);
        let id = PostId(request.into_inner().post_id);
        let post = self.service.public(id).await;
        let post = post.map_err(|error| failed(&error, &rid))?;
        let mut marked = self.marked(viewer, &[post]).await;
        Ok(Response::new(marked.remove(0)))
    }

    async fn like(&self, request: Request<PostAction>) -> Result<Response<LikeReply>, Status> {
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

    async fn edit_post(&self, request: Request<EditPostRequest>) -> Result<Response<Post>, Status> {
        self.edit(request).await
    }

    async fn delete_post(&self, request: Request<PostAction>) -> Result<Response<Changed>, Status> {
        self.delete(request).await
    }

    async fn feed(&self, request: Request<FeedRequest>) -> Result<Response<FeedReply>, Status> {
        let (viewer, users, rid) = self.actor(&request)?;
        let ask = request.into_inner();
        let feed = self
            .service
            .feed(&users, viewer, ask.page as usize, ask.size as usize)
            .await;
        let posts = feed.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(FeedReply {
            posts: self.marked(Some(viewer), &posts).await,
            next: String::new(),
        }))
    }

    async fn author_posts(
        &self,
        request: Request<AuthorPostsRequest>,
    ) -> Result<Response<FeedReply>, Status> {
        let (author, limit) =
            dto::author_page(request.get_ref()).map_err(|errors| invalid(&errors))?;
        let viewer = caller_of(&request).ok().map(UserId);
        let posts = self.service.by_author(author, limit).await;
        Ok(Response::new(FeedReply {
            posts: self.marked(viewer, &posts).await,
            next: String::new(),
        }))
    }

    async fn comment(
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

    async fn trends(
        &self,
        request: Request<TrendsRequest>,
    ) -> Result<Response<TrendsReply>, Status> {
        let limit = request.into_inner().limit as usize;
        let trends = self.service.trends(limit).await;
        Ok(Response::new(TrendsReply {
            trends: trends.iter().map(dto::trend).collect(),
        }))
    }

    type WatchStream = ReceiverStream<Result<FeedEvent, Status>>;

    async fn watch(
        &self,
        request: Request<WatchRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        self.stream(request).await
    }

    async fn get_posts(&self, request: Request<PostsByIds>) -> Result<Response<FeedReply>, Status> {
        self.hydrate_posts(request).await
    }

    async fn author_counters(
        &self,
        request: Request<AuthorPostsRequest>,
    ) -> Result<Response<AuthorTotals>, Status> {
        self.count_author(request).await
    }

    async fn daily_stats(
        &self,
        request: Request<DailyRequest>,
    ) -> Result<Response<DailyReply>, Status> {
        self.daily_of(request).await
    }

    async fn thread(
        &self,
        request: Request<ThreadRequest>,
    ) -> Result<Response<ThreadReply>, Status> {
        self.thread_of(request).await
    }

    async fn top_authors(
        &self,
        request: Request<TopRequest>,
    ) -> Result<Response<TopReply>, Status> {
        self.top_of(request).await
    }

    async fn pin(&self, request: Request<PostAction>) -> Result<Response<Changed>, Status> {
        self.pin_post(request, true).await
    }

    async fn unpin(&self, request: Request<PostAction>) -> Result<Response<Changed>, Status> {
        self.pin_post(request, false).await
    }

    async fn pinned_posts(
        &self,
        request: Request<AuthorPostsRequest>,
    ) -> Result<Response<FeedReply>, Status> {
        self.pinned_of(request).await
    }

    async fn like_comment(
        &self,
        request: Request<CommentAction>,
    ) -> Result<Response<CommentLikeReply>, Status> {
        self.like_comment_of(request).await
    }
}
