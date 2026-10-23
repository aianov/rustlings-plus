use std::future::ready;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

use futures::{Stream, StreamExt};

use super::{App, AppError};
use crate::domain::{PostId, UserId};
use crate::feed::{self, FeedEvent};
use crate::ports::{FeedSlice, PostItem};

type Boxed<T> = Pin<Box<dyn Stream<Item = T> + Send>>;

struct Watching {
    count: Arc<AtomicUsize>,
}

impl Watching {
    fn enter(count: &Arc<AtomicUsize>) -> Watching {
        count.fetch_add(1, Ordering::SeqCst);
        Watching {
            count: Arc::clone(count),
        }
    }
}

impl Drop for Watching {
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::SeqCst);
    }
}

struct Tracked<T> {
    inner: Boxed<T>,
    _watching: Watching,
}

impl<T> Stream for Tracked<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        self.inner.as_mut().poll_next(cx)
    }
}

impl App {
    pub async fn feed_page(
        &self,
        viewer: UserId,
        before: Option<PostId>,
        size: usize,
    ) -> Result<FeedSlice, AppError> {
        let posts = &self.inner.repos.posts;
        Ok(posts.feed_page(viewer, before, size).await?)
    }

    pub async fn feed_item(&self, viewer: UserId, post: PostId) -> Option<PostItem> {
        let mut item = self.inner.repos.posts.feed_item(viewer, post).await?;
        if let Some(author) = self.profile(item.author).await {
            item.author_name = author.shared_name();
        }
        Some(item)
    }

    pub fn feed_stream(
        &self,
        viewer: UserId,
    ) -> impl Stream<Item = Result<PostItem, AppError>> + Send + use<> {
        let size = self.inner.config.feed_page.max(1);
        let app = self.clone();
        let pages = feed::pages(move |before| {
            let app = app.clone();
            async move { app.feed_page(viewer, before, size).await }
        });
        let app = self.clone();
        let lookup = move |author| {
            let app = app.clone();
            async move { app.profile(author).await }
        };
        feed::with_authors(pages, lookup, self.inner.config.feed_lookups)
    }

    pub fn live(&self, viewer: UserId) -> impl Stream<Item = PostItem> + Send + use<> {
        let watching = Watching::enter(&self.inner.watchers);
        let app = self.clone();
        let posts = feed::new_posts(self.events(), self.inner.shutdown.token())
            .then(move |post| {
                let app = app.clone();
                async move { app.feed_item(viewer, post).await }
            })
            .filter_map(ready);
        Tracked {
            inner: Box::pin(posts),
            _watching: watching,
        }
    }

    pub fn watch(
        &self,
        viewer: UserId,
        recent: usize,
    ) -> impl Stream<Item = Result<FeedEvent, AppError>> + Send + use<> {
        let live = self.live(viewer);
        feed::watch(self.feed_stream(viewer).take(recent), live)
    }

    pub fn watchers(&self) -> usize {
        self.inner.watchers.load(Ordering::SeqCst)
    }
}
