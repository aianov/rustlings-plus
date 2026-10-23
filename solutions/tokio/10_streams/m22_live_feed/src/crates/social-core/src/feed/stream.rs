use std::future::{Future, ready};
use std::sync::Arc;

use futures::stream::{self, Stream, StreamExt, TryStreamExt};
use service_runtime::CancellationToken;

use crate::app::AppError;
use crate::domain::{PostId, User, UserId};
use crate::events::{Event, EventStream};
use crate::ports::{FeedSlice, PostItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedEvent {
    History(PostItem),
    UpToDate,
    Live(PostItem),
}

pub fn pages<F, Fut>(fetch: F) -> impl Stream<Item = Result<PostItem, AppError>>
where
    F: FnMut(Option<PostId>) -> Fut,
    Fut: Future<Output = Result<FeedSlice, AppError>>,
{
    stream::unfold((fetch, Some(None)), |(mut fetch, cursor)| async move {
        let before = cursor?;
        Some(match fetch(before).await {
            Ok(page) => (Ok(page.posts), (fetch, page.next.map(Some))),
            Err(error) => (Err(error), (fetch, None)),
        })
    })
    .map_ok(|posts| stream::iter(posts.into_iter().map(Ok)))
    .try_flatten()
}

pub fn with_authors<S, F, Fut>(
    posts: S,
    lookup: F,
    lookups: usize,
) -> impl Stream<Item = Result<PostItem, AppError>>
where
    S: Stream<Item = Result<PostItem, AppError>>,
    F: Fn(UserId) -> Fut,
    Fut: Future<Output = Option<Arc<User>>>,
{
    posts
        .map(move |post| {
            let author = post.as_ref().ok().map(|post| lookup(post.author));
            async move {
                let mut post = post?;
                let user = match author {
                    Some(author) => author.await,
                    None => None,
                };
                if let Some(user) = user {
                    post.author_name = user.shared_name();
                }
                Ok(post)
            }
        })
        .buffered(lookups.max(1))
}

pub fn new_posts(events: EventStream, stop: CancellationToken) -> impl Stream<Item = PostId> {
    stream::unfold((events, stop), |(mut events, stop)| async move {
        let event = tokio::select! {
            biased;
            () = stop.cancelled() => None,
            event = events.next_event() => event,
        }?;
        Some((event, (events, stop)))
    })
    .filter_map(|event| {
        ready(match *event {
            Event::NewPost { post, .. } => Some(post),
            Event::Liked { .. } | Event::Followed { .. } | Event::MessageSent { .. } => None,
        })
    })
}

enum Step {
    Old(Result<PostItem, AppError>),
    Caught,
    New(PostItem),
}

pub fn watch<H, L>(history: H, live: L) -> impl Stream<Item = Result<FeedEvent, AppError>>
where
    H: Stream<Item = Result<PostItem, AppError>>,
    L: Stream<Item = PostItem>,
{
    history
        .map(Step::Old)
        .chain(stream::once(ready(Step::Caught)))
        .chain(live.map(Step::New))
        .scan(None, |top, step| ready(Some(fresh(top, step))))
        .filter_map(ready)
}

fn fresh(top: &mut Option<PostId>, step: Step) -> Option<Result<FeedEvent, AppError>> {
    match step {
        Step::Old(Ok(post)) => {
            *top = Some(top.map_or(post.id, |top| top.max(post.id)));
            Some(Ok(FeedEvent::History(post)))
        }
        Step::Old(Err(error)) => Some(Err(error)),
        Step::Caught => Some(Ok(FeedEvent::UpToDate)),
        Step::New(post) if top.is_some_and(|top| post.id <= top) => None,
        Step::New(post) => Some(Ok(FeedEvent::Live(post))),
    }
}
