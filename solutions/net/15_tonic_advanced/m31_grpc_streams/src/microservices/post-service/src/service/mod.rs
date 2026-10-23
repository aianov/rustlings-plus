use std::future::Future;
use std::time::Duration;

use futures::StreamExt;
use grpc_kit::{Stamp, caller_of};
use proto::internal;
use proto::social::v1::feed_event::Kind;
use proto::social::v1::posts_server::Posts;
use proto::social::v1::{
    CaughtUp, Changed, CommentReply, CommentRequest, CreatePostRequest, FeedEvent, FeedReply,
    FeedRequest, GetPostRequest, LikeReply, Post, PostAction, Trend, TrendsReply, TrendsRequest,
    WatchRequest,
};
use service_runtime::CancellationToken;
use social_core::{
    App, AppError, DomainError, FeedEvent as CoreEvent, MAX_TRENDS, PostId, PostItem, UserId,
    Visibility,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use crate::mirror::{Mirror, failed};

pub const RECENT_MAX: u32 = 100;

pub const SYNC_BUDGET: Duration = Duration::from_secs(1);

pub struct PostsService {
    app: App,
    mirror: Mirror,
    stop: CancellationToken,
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

fn event(core: CoreEvent) -> FeedEvent {
    let kind = match core {
        CoreEvent::History(item) => Kind::History(post(&item)),
        CoreEvent::UpToDate => Kind::CaughtUp(CaughtUp {}),
        CoreEvent::Live(item) => Kind::Live(post(&item)),
    };
    FeedEvent { kind: Some(kind) }
}

async fn finish<T, F>(work: F, rid: &str) -> Result<T, Status>
where
    T: Send + 'static,
    F: Future<Output = Result<T, AppError>> + Send + 'static,
{
    match tokio::spawn(work).await {
        Ok(done) => done.map_err(|error| failed(error, rid)),
        Err(..) => {
            eprintln!("post-service [{rid}]: задача записи упала");
            Err(internal())
        }
    }
}

impl PostsService {
    pub fn new(app: App, mirror: Mirror, stop: CancellationToken) -> PostsService {
        PostsService { app, mirror, stop }
    }

    async fn item(&self, id: PostId, rid: &str) -> Result<PostItem, Status> {
        match self.app.post(id).await {
            Some(item) => Ok(item),
            None => Err(failed(DomainError::UnknownPost(id), rid)),
        }
    }

    async fn actor<T>(&self, request: &Request<T>) -> Result<(UserId, Stamp), Status> {
        let user = UserId(caller_of(request)?);
        let stamp = Stamp::follow(request);
        self.mirror.ensure(&self.app, user, &stamp).await?;
        Ok((user, stamp))
    }
}

#[tonic::async_trait]
impl Posts for PostsService {
    async fn create_post(
        &self,
        request: Request<CreatePostRequest>,
    ) -> Result<Response<Post>, Status> {
        let author = UserId(caller_of(&request)?);
        let stamp = Stamp::follow(&request);
        let rid = stamp.request_id().to_string();
        self.mirror.profile(author, &stamp).await?;
        self.mirror.ensure(&self.app, author, &stamp).await?;
        let text = request.into_inner().text;
        let app = self.app.clone();
        let publish = async move { app.publish(author, &text, Visibility::Public).await };
        let id = finish(publish, &rid).await?;
        let item = self.item(id, &rid).await?;
        Ok(Response::new(post(&item)))
    }

    async fn get_post(&self, request: Request<GetPostRequest>) -> Result<Response<Post>, Status> {
        let rid = Stamp::follow(&request).request_id().to_string();
        let id = PostId(request.into_inner().post_id);
        match self.app.public_post(id).await {
            Some(item) => Ok(Response::new(post(&item))),
            None => Err(failed(DomainError::UnknownPost(id), &rid)),
        }
    }

    async fn like(&self, request: Request<PostAction>) -> Result<Response<LikeReply>, Status> {
        let (user, stamp) = self.actor(&request).await?;
        let rid = stamp.request_id().to_string();
        let id = PostId(request.into_inner().post_id);
        let app = self.app.clone();
        let changed = finish(async move { app.like(user, id).await }, &rid).await?;
        let author = self.item(id, &rid).await?.author;
        Ok(Response::new(LikeReply {
            changed,
            author_id: author.0,
        }))
    }

    async fn delete_post(&self, request: Request<PostAction>) -> Result<Response<Changed>, Status> {
        let (user, stamp) = self.actor(&request).await?;
        let id = PostId(request.into_inner().post_id);
        let app = self.app.clone();
        let deleted = async move { app.delete_post(user, id).await };
        let changed = finish(deleted, stamp.request_id()).await?;
        Ok(Response::new(Changed { changed }))
    }

    async fn feed(&self, request: Request<FeedRequest>) -> Result<Response<FeedReply>, Status> {
        let (viewer, stamp) = self.actor(&request).await?;
        self.mirror.sync_viewer(&self.app, viewer, &stamp).await?;
        let ask = request.into_inner();
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
        let (author, stamp) = self.actor(&request).await?;
        let ask = request.into_inner();
        let app = self.app.clone();
        let commented = async move { app.comment(author, PostId(ask.post_id), &ask.text).await };
        let comment = finish(commented, stamp.request_id()).await?;
        Ok(Response::new(CommentReply {
            comment_id: comment.0,
        }))
    }

    async fn trends(
        &self,
        request: Request<TrendsRequest>,
    ) -> Result<Response<TrendsReply>, Status> {
        let rid = Stamp::follow(&request).request_id().to_string();
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

    type WatchStream = ReceiverStream<Result<FeedEvent, Status>>;

    async fn watch(
        &self,
        request: Request<WatchRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        let viewer = UserId(caller_of(&request)?);
        let stamp = Stamp::follow(&request).within(SYNC_BUDGET);
        self.mirror.ensure(&self.app, viewer, &stamp).await?;
        self.mirror.sync_viewer(&self.app, viewer, &stamp).await?;
        let recent = match request.into_inner().recent {
            0 => 20,
            recent => recent.min(RECENT_MAX),
        };
        let events = self.app.watch(viewer, recent as usize);
        let rid = stamp.request_id().to_string();
        let (tx, rx) = mpsc::channel(16);
        let stop = self.stop.clone();
        tokio::spawn(async move {
            let mut events = Box::pin(events);
            loop {
                let next = tokio::select! {
                    next = events.next() => next,
                    () = tx.closed() => return,
                    () = stop.cancelled() => return,
                };
                let sent = match next {
                    Some(Ok(core)) => tx.send(Ok(event(core))).await,
                    Some(Err(error)) => {
                        let _ = tx.send(Err(failed(error, &rid))).await;
                        return;
                    }
                    None => return,
                };
                if sent.is_err() {
                    return;
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
