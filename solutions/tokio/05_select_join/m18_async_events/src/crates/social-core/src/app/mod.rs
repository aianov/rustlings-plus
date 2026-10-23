mod card;
mod config;
mod error;
mod fanout;
mod heavy;
mod import;
mod settings;
mod trends;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::runtime::Handle;
use tokio::sync::{Semaphore, broadcast, mpsc, watch};

use crate::chat::Message;
use crate::domain::{CommentId, DomainError, MAX_POST_LEN, PostId, User, UserId, Visibility};
use crate::error::Error;
use crate::events::{Event, EventStream, SubId, Topic};
use crate::network::Network;
use crate::notify::{Inbox, Notification, Notifiers};
use crate::ports::{PostStore, UserStore};
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

const NOTIFIED: [Topic; 3] = [Topic::Followed, Topic::Liked, Topic::MessageSent];

struct Inner<U, P> {
    network: Mutex<Network<U, P>>,
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

pub struct App<U, P> {
    inner: Arc<Inner<U, P>>,
}

impl<U, P> Clone for App<U, P> {
    fn clone(&self) -> App<U, P> {
        App {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<U, P> App<U, P>
where
    U: UserStore + Send + 'static,
    P: PostStore + Send + 'static,
{
    pub fn new(network: Network<U, P>) -> crate::Result<App<U, P>> {
        App::start(network, Notifiers::new(), AppConfig::default())
    }

    pub fn with_notifiers(
        network: Network<U, P>,
        notifiers: Notifiers,
    ) -> crate::Result<App<U, P>> {
        App::start(network, notifiers, AppConfig::default())
    }

    pub fn start(
        mut network: Network<U, P>,
        mut notifiers: Notifiers,
        config: AppConfig,
    ) -> crate::Result<App<U, P>> {
        let runtime = Handle::try_current().map_err(|_| Error::NoRuntime)?;
        let inbox = Arc::new(Inbox::new(config.shards));
        notifiers.add(inbox.clone());
        let counters = Arc::new(Counters::default());
        notifiers.add(counters.clone());
        for topic in NOTIFIED {
            let notifiers = notifiers.clone();
            network.subscribe(topic, move |event| notifiers.deliver(event));
        }
        let (events, _) = broadcast::channel(config.events.max(1));
        let publisher = events.clone();
        network.subscribe(Topic::All, move |event| {
            let _ = publisher.send(Arc::new(*event));
        });
        let (stop, stopped) = watch::channel(());
        let (settings, _) = watch::channel(Settings::default());
        let (trends, requests) = mpsc::channel(8);
        let fanout = Fanout::start(&runtime, notifiers, counters, config.queue, stopped.clone());
        let inner = Arc::new(Inner {
            network: Mutex::new(network),
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

    pub async fn register(&self, name: &str) -> Result<UserId, AppError> {
        Ok(self.lock().register(name)?)
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
        let (post, audience) = {
            let mut network = self.lock();
            let post = network.publish(author, text, visibility)?;
            (post, network.audience(post))
        };
        self.inner.fanout.published(author, post, audience).await?;
        Ok(post)
    }

    pub async fn follow(&self, who: UserId, whom: UserId) -> Result<bool, AppError> {
        Ok(self.lock().follow(who, whom)?)
    }

    pub async fn like(&self, user: UserId, post: PostId) -> Result<bool, AppError> {
        Ok(self.lock().like(user, post)?)
    }

    pub async fn mute(&self, who: UserId, whom: UserId) -> Result<bool, AppError> {
        Ok(self.lock().mute(who, whom)?)
    }

    pub async fn delete_post(&self, who: UserId, post: PostId) -> Result<bool, AppError> {
        Ok(self.lock().delete_post(who, post)?)
    }

    pub async fn comment(
        &self,
        author: UserId,
        post: PostId,
        text: &str,
    ) -> Result<CommentId, AppError> {
        Ok(self.lock().comment(author, post, text)?)
    }

    pub async fn reply(
        &self,
        author: UserId,
        parent: CommentId,
        text: &str,
    ) -> Result<CommentId, AppError> {
        Ok(self.lock().reply(author, parent, text)?)
    }

    pub async fn send_message(
        &self,
        sender: UserId,
        recipient: UserId,
        text: &str,
    ) -> Result<Message, AppError> {
        let message = self.lock().send_message(sender, recipient, text)?.clone();
        Ok(message)
    }

    pub fn subscribe(&self, topic: Topic, handler: impl FnMut(&Event) + Send + 'static) -> SubId {
        self.lock().subscribe(topic, handler)
    }

    pub fn unsubscribe(&self, id: SubId) -> bool {
        self.lock().unsubscribe(id)
    }

    pub fn read<R>(&self, f: impl FnOnce(&Network<U, P>) -> R) -> R {
        f(&self.lock())
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

    pub fn profile(&self, id: UserId) -> Option<Arc<User>> {
        if let Some(user) = self.inner.profiles.get(&id) {
            return Some(user);
        }
        let user = self.read(|network| network.user(id).cloned())?;
        let kept = self.inner.profiles.with(&id, |cache| {
            Arc::clone(cache.entry(id).or_insert_with(|| Arc::new(user)))
        });
        Some(kept)
    }

    fn lock(&self) -> MutexGuard<'_, Network<U, P>> {
        self.inner
            .network
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}
