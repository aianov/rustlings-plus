mod card;
mod config;
mod error;
mod fanout;
mod heavy;
mod import;
mod journal;
mod live;
mod queries;
mod settings;
mod trends;
mod writes;

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

use service_runtime::{Console, Restart, Shutdown, ShutdownReport};
use tokio::runtime::Handle;
use tokio::sync::{Semaphore, broadcast, watch};

use crate::domain::{User, UserId};
use crate::error::Error;
use crate::events::{Event, EventStream};
use crate::journal::Journal;
use crate::notify::{Inbox, Notification, Notifiers};
use crate::ports::Repos;
use crate::shards::Shards;
use fanout::{Batching, Counters, Fanout};
use journal::Recorder;
use trends::{Ticker, TrendCounters};

pub use card::{CARD_POSTS, Card};
pub use config::AppConfig;
pub use error::{AppError, ImportError, Op};
pub use fanout::Stats;
pub use import::Imported;
pub use settings::Settings;
pub use trends::{MAX_TRENDS, TrendsHandle};

struct Inner {
    repos: Repos,
    notifiers: Notifiers,
    inbox: Arc<Inbox>,
    fanout: Fanout,
    profiles: Shards<UserId, Arc<User>>,
    heavy: Arc<Semaphore>,
    trends: TrendsHandle,
    trend_counters: Arc<TrendCounters>,
    events: broadcast::Sender<Arc<Event>>,
    settings: watch::Sender<Settings>,
    journal: Recorder,
    watchers: Arc<AtomicUsize>,
    config: AppConfig,
    runtime: Handle,
    shutdown: Shutdown,
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.shutdown.token().cancel();
    }
}

#[derive(Clone)]
pub struct App {
    inner: Arc<Inner>,
}

impl App {
    pub fn with_repos(repos: Repos, notifiers: Notifiers, config: AppConfig) -> crate::Result<App> {
        App::assemble(repos, notifiers, config, None)
    }

    pub fn journaled(
        repos: Repos,
        notifiers: Notifiers,
        config: AppConfig,
        journal: Journal,
    ) -> crate::Result<App> {
        App::assemble(repos, notifiers, config, Some(journal))
    }

    fn assemble(
        repos: Repos,
        mut notifiers: Notifiers,
        config: AppConfig,
        journal: Option<Journal>,
    ) -> crate::Result<App> {
        let runtime = Handle::try_current().map_err(|_| Error::NoRuntime)?;
        let inbox = Arc::new(Inbox::new(config.shards));
        notifiers.add(inbox.clone());
        let counters = Arc::new(Counters::default());
        notifiers.add(counters.clone());
        let (events, _) = broadcast::channel(config.events.max(1));
        let (settings, _) = watch::channel(Settings::default());
        let mut shutdown = Shutdown::with_console(Console::with_threshold(config.long_poll));
        let sources = shutdown.stage("sources");
        let handlers = shutdown.stage("handlers");
        let trends_stage = shutdown.stage("trends");
        let journal = match journal {
            Some(journal) => {
                let stage = shutdown.stage("journal");
                let every = config.journal_flush_every;
                Recorder::start(&stage, journal, every, config.queue)
            }
            None => Recorder::off(),
        };
        let fanout = Fanout::start(
            &handlers,
            &shutdown.token(),
            notifiers.clone(),
            counters,
            (config.delivery_workers, config.queue),
            Batching {
                size: config.batch_size,
                window: config.batch_window,
            },
            Restart {
                pause: config.restart_pause,
                max_restarts: config.max_restarts,
            },
        );
        let (trends, actor) = trends::trends_actor();
        let trend_counters = Arc::new(TrendCounters::default());
        let inner = Arc::new(Inner {
            repos,
            notifiers,
            inbox,
            fanout,
            profiles: Shards::new(config.shards),
            heavy: Arc::new(Semaphore::new(config.heavy_limit.max(1))),
            trends: trends.clone(),
            trend_counters: Arc::clone(&trend_counters),
            events,
            settings,
            journal,
            watchers: Arc::default(),
            config,
            runtime,
            shutdown,
        });
        let ticker = Ticker {
            trends,
            counters: Arc::clone(&trend_counters),
            settings: inner.settings.subscribe(),
            events: EventStream::new(inner.events.subscribe()),
            every: config.trends_every,
            quiet: config.trends_debounce,
        };
        trends_stage.spawn(
            "trends-actor",
            actor.run(Arc::downgrade(&inner), trend_counters, trends_stage.clone()),
        );
        sources.spawn("trends-ticker", ticker.run(sources.clone()));
        Ok(App { inner })
    }

    pub async fn close(&self, deadline: Duration) -> ShutdownReport {
        self.inner.shutdown.close(deadline).await
    }

    pub fn console(&self) -> &Console {
        self.inner.shutdown.console()
    }

    pub fn is_closed(&self) -> bool {
        self.inner.shutdown.token().is_cancelled()
    }

    pub fn restarts(&self) -> u64 {
        self.inner.fanout.restarts()
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

    pub async fn inbox(&self, user: UserId) -> Result<Vec<Notification>, AppError> {
        self.flush().await?;
        Ok(self.inner.inbox.inbox(user))
    }

    pub async fn flush(&self) -> Result<(), AppError> {
        self.inner.fanout.flush(self.inner.config.op_timeout).await
    }

    pub fn stats(&self) -> Stats {
        self.inner
            .fanout
            .stats(self.inner.trend_counters.refreshes())
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
