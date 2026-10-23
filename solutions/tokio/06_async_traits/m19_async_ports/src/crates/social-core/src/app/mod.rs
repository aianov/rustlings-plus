mod card;
mod config;
mod error;
mod fanout;
mod heavy;
mod import;
mod queries;
mod settings;
mod trends;

use std::sync::Arc;

use tokio::runtime::Handle;
use tokio::sync::{Semaphore, broadcast, mpsc, watch};

use crate::chat::Message;
use crate::domain::{CommentId, DomainError, MAX_POST_LEN, PostId, User, UserId, Visibility};
use crate::error::Error;
use crate::events::{Event, EventStream};
use crate::notify::{Inbox, Notification, Notifiers};
use crate::ports::Repos;
use crate::shards::Shards;
use fanout::{Counters, Fanout};
use trends::{Board, Ticker};

pub use card::{CARD_POSTS, Card};
pub use config::AppConfig;
pub use error::{AppError, ImportError, Op};
pub use fanout::Stats;
pub use import::Imported;
pub use settings::Settings;
pub use trends::MAX_TRENDS;

struct Inner {
    repos: Repos,
    notifiers: Notifiers,
    inbox: Arc<Inbox>,
    fanout: Fanout,
    profiles: Shards<UserId, Arc<User>>,
    heavy: Arc<Semaphore>,
    board: Board,
    events: broadcast::Sender<Arc<Event>>,
    settings: watch::Sender<Settings>,
    trends: trends::Requests,
    config: AppConfig,
    runtime: Handle,
    _stop: watch::Sender<()>,
}

#[derive(Clone)]
pub struct App {
    inner: Arc<Inner>,
}

impl App {
    pub fn with_repos(
        repos: Repos,
        mut notifiers: Notifiers,
        config: AppConfig,
    ) -> crate::Result<App> {
        let runtime = Handle::try_current().map_err(|_| Error::NoRuntime)?;
        let inbox = Arc::new(Inbox::new(config.shards));
        notifiers.add(inbox.clone());
        let counters = Arc::new(Counters::default());
        notifiers.add(counters.clone());
        let (events, _) = broadcast::channel(config.events.max(1));
        let (stop, stopped) = watch::channel(());
        let (settings, _) = watch::channel(Settings::default());
        let (trends, requests) = mpsc::channel(8);
        let fanout = Fanout::start(
            &runtime,
            notifiers.clone(),
            counters,
            config.queue,
            stopped.clone(),
        );
        let inner = Arc::new(Inner {
            repos,
            notifiers,
            inbox,
            fanout,
            profiles: Shards::new(config.shards),
            heavy: Arc::new(Semaphore::new(config.heavy_limit.max(1))),
            board: Board::default(),
            events,
            settings,
            trends,
            config,
            runtime,
            _stop: stop,
        });
        let ticker = Ticker {
            app: Arc::downgrade(&inner),
            requests,
            stop: stopped,
            settings: inner.settings.subscribe(),
            events: EventStream::new(inner.events.subscribe()),
            every: config.trends_every,
            quiet: config.trends_debounce,
        };
        inner.runtime.spawn(ticker.run());
        Ok(App { inner })
    }

    pub fn config(&self) -> &AppConfig {
        &self.inner.config
    }

    pub fn handle(&self) -> &Handle {
        &self.inner.runtime
    }

    pub fn events(&self) -> EventStream {
        EventStream::new(self.inner.events.subscribe())
    }

    fn emit(&self, event: Event) {
        self.inner.notifiers.deliver(&event);
        let _ = self.inner.events.send(Arc::new(event));
    }

    pub async fn register(&self, name: &str) -> Result<UserId, AppError> {
        Ok(self.inner.repos.users.register(name.to_string()).await?)
    }

    pub async fn publish(
        &self,
        author: UserId,
        text: &str,
        visibility: Visibility,
    ) -> Result<PostId, AppError> {
        let limit = self.settings().post_limit;
        if limit < MAX_POST_LEN && text.trim().chars().count() > limit {
            return Err(DomainError::PostOverLimit(limit).into());
        }
        let published = self
            .inner
            .repos
            .posts
            .publish(author, text.to_string(), visibility)
            .await?;
        let post = published.post;
        self.emit(Event::NewPost { author, post });
        self.inner
            .fanout
            .published(author, post, published.audience)
            .await?;
        Ok(post)
    }

    pub async fn follow(&self, who: UserId, whom: UserId) -> Result<bool, AppError> {
        let added = self.inner.repos.users.follow(who, whom).await?;
        if added {
            self.emit(Event::Followed { who, whom });
        }
        Ok(added)
    }

    pub async fn like(&self, user: UserId, post: PostId) -> Result<bool, AppError> {
        let liked = self.inner.repos.posts.like(user, post).await?;
        if liked.changed {
            self.emit(Event::Liked {
                by: user,
                post,
                author: liked.author,
            });
        }
        Ok(liked.changed)
    }

    pub async fn mute(&self, who: UserId, whom: UserId) -> Result<bool, AppError> {
        Ok(self.inner.repos.users.mute(who, whom).await?)
    }

    pub async fn delete_post(&self, who: UserId, post: PostId) -> Result<bool, AppError> {
        Ok(self.inner.repos.posts.delete(who, post).await?)
    }

    pub async fn comment(
        &self,
        author: UserId,
        post: PostId,
        text: &str,
    ) -> Result<CommentId, AppError> {
        let posts = &self.inner.repos.posts;
        Ok(posts.comment(author, post, text.to_string()).await?)
    }

    pub async fn reply(
        &self,
        author: UserId,
        parent: CommentId,
        text: &str,
    ) -> Result<CommentId, AppError> {
        let posts = &self.inner.repos.posts;
        Ok(posts.reply(author, parent, text.to_string()).await?)
    }

    pub async fn send_message(
        &self,
        sender: UserId,
        recipient: UserId,
        text: &str,
    ) -> Result<Message, AppError> {
        let chats = &self.inner.repos.chats;
        let message = chats.send(sender, recipient, text.to_string()).await?;
        self.emit(Event::MessageSent {
            from: sender,
            to: recipient,
        });
        Ok(message)
    }

    pub async fn inbox(&self, user: UserId) -> Result<Vec<Notification>, AppError> {
        self.flush().await?;
        Ok(self.inner.inbox.inbox(user))
    }

    pub async fn flush(&self) -> Result<(), AppError> {
        self.inner.fanout.flush(self.inner.config.op_timeout).await
    }

    pub fn stats(&self) -> Stats {
        self.inner.fanout.stats(self.inner.board.refreshes())
    }

    pub async fn profile(&self, id: UserId) -> Option<Arc<User>> {
        if let Some(user) = self.inner.profiles.get(&id) {
            return Some(user);
        }
        let user = self.inner.repos.users.user(id).await?;
        let kept = self.inner.profiles.with(&id, |cache| {
            Arc::clone(cache.entry(id).or_insert_with(|| Arc::new(user)))
        });
        Some(kept)
    }
}
