mod drafts;

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use contracts::{ErrorCode, Origin, PostId, Role, UserId};
use grpc_kit::{Stamp, caller_of, role_of};
use proto::social::v1::posts_server::Posts;
use proto::social::v1::{
    AuthorPostsRequest, Changed, CommentAction, CommentReply, CommentRequest, CreatePostRequest,
    DraftAction, DraftReply, DraftRequest, EditPostRequest, FeedEvent, FeedReply, FeedRequest,
    GetPostRequest, LikeReply, Post, PostAction, TrendsReply, TrendsRequest, WatchRequest,
};
use proto::{internal, invalid, request_id, to_status};
use service_runtime::CancellationToken;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;
use tonic::{Request, Response, Status};

use crate::client::{GrpcUsers, Previewer};
use crate::common::Actor;
use crate::dto;
use crate::service::{MAX_KEY, PostError, PostService, good_key};

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

    async fn get_post(&self, request: Request<GetPostRequest>) -> Result<Response<Post>, Status> {
        let rid = request_id(request.metadata());
        let id = PostId(request.into_inner().post_id);
        let post = self.service.public(id).await;
        let post = post.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(dto::post(&post)))
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

    async fn delete_post(&self, request: Request<PostAction>) -> Result<Response<Changed>, Status> {
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

    async fn feed(&self, request: Request<FeedRequest>) -> Result<Response<FeedReply>, Status> {
        let (viewer, users, rid) = self.actor(&request)?;
        let ask = request.into_inner();
        let feed = self
            .service
            .feed(&users, viewer, ask.page as usize, ask.size as usize)
            .await;
        let posts = feed.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(FeedReply {
            posts: posts.iter().map(dto::post).collect(),
            next: String::new(),
        }))
    }

    async fn author_posts(
        &self,
        request: Request<AuthorPostsRequest>,
    ) -> Result<Response<FeedReply>, Status> {
        let (author, limit) =
            dto::author_page(request.get_ref()).map_err(|errors| invalid(&errors))?;
        let posts = self.service.by_author(author, limit).await;
        Ok(Response::new(FeedReply {
            posts: posts.iter().map(dto::post).collect(),
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
        let post = PostId(request.into_inner().post_id);
        let commented = async move { service.comment(author, post, text, &origin).await };
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
        let viewer = UserId(caller_of(&request)?);
        let stamp = Stamp::follow(&request).within(SYNC_BUDGET);
        let rid = stamp.request_id().to_string();
        let users = GrpcUsers::new(self.users.clone(), stamp);
        let recent = match request.into_inner().recent {
            0 => 20,
            recent => recent.min(RECENT_MAX),
        };
        let watch = self.service.watch(&users, viewer, recent as usize).await;
        let mut watch = watch.map_err(|error| failed(&error, &rid))?;
        let (tx, rx) = mpsc::channel(16);
        let stop = self.stop.clone();
        tokio::spawn(async move {
            let mut events: Vec<FeedEvent> = watch.history().iter().map(dto::history).collect();
            events.push(dto::caught_up());
            for event in events {
                if tx.send(Ok(event)).await.is_err() {
                    return;
                }
            }
            loop {
                let next = tokio::select! {
                    next = watch.next() => next,
                    () = tx.closed() => return,
                    () = stop.cancelled() => return,
                };
                let Some(post) = next else {
                    return;
                };
                if tx.send(Ok(dto::live(&post))).await.is_err() {
                    return;
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
