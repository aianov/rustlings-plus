use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use contracts::{DomainEvent, SessionRevoked, Topics, UserId};
use nats_engine::js::{Bucket, StreamSpec, ensure_stream, history, open_bucket};
use nats_engine::{Bus, BusError, Pattern, Subject, Subscription, fresh_id};
use service_runtime::CancellationToken;
use tokio::sync::Mutex;

use resp_client::Client;

use crate::common::RedisPresence;
use crate::common::{ConnId, NAME, Outbox, Principal, Registration, Registry};

use super::revoked::Revoked;
use super::subs::{Subs, message};

pub const CATCH_UP_PAGE: usize = 100;

pub struct Realtime {
    bus: Bus,
    topics: Topics,
    instance: String,
    registry: Arc<Registry>,
    bucket: Bucket,
    users: Mutex<HashMap<UserId, Subs>>,
    revoked: Revoked,
    redis: Option<RedisPresence>,
}

pub struct Seat {
    realtime: Arc<Realtime>,
    registration: Registration,
}

fn pattern(words: &[String]) -> Result<Pattern, BusError> {
    Pattern::parse(&words.join(".")).map_err(|error| BusError::Encode(error.to_string()))
}

impl Realtime {
    pub async fn start(
        bus: Bus,
        topics: Topics,
        registry: Arc<Registry>,
        ttl: Duration,
        closing: CancellationToken,
        redis: Option<Client>,
    ) -> Result<Arc<Realtime>, BusError> {
        let chat = StreamSpec::new(&topics.chat_stream(), topics.chat_subjects())
            .republish(&topics.chat_all().join("."), &topics.live_all().join("."));
        ensure_stream(&bus, &chat).await?;
        let bucket = open_bucket(&bus, &topics.presence(), ttl).await?;
        let revocations = bus
            .subscribe::<SessionRevoked>(&pattern(&topics.revoked())?)
            .await?;
        let instance = fresh_id("gw");
        let redis = redis.map(|redis| RedisPresence::new(redis, ttl, &instance));
        let realtime = Arc::new(Realtime {
            instance,
            redis,
            bus,
            topics,
            registry,
            bucket,
            users: Mutex::default(),
            revoked: Revoked::default(),
        });
        tokio::spawn(Arc::clone(&realtime).listen(revocations, closing.clone()));
        tokio::spawn(Arc::clone(&realtime).beat(ttl / 3, closing));
        Ok(realtime)
    }

    fn key(&self, user: UserId, conn: ConnId) -> String {
        format!("{user}.{}-{conn}", self.instance)
    }

    pub async fn online_many(&self, users: &[UserId]) -> Vec<bool> {
        if let Some(redis) = &self.redis {
            return redis.online_many(users).await;
        }
        let mut found = Vec::with_capacity(users.len());
        for user in users {
            found.push(self.online(*user).await.unwrap_or(false));
        }
        found
    }

    pub fn is_revoked(&self, session: &str) -> bool {
        self.revoked.contains(session)
    }

    pub fn remembered(&self) -> usize {
        self.revoked.len()
    }

    pub async fn subscribed(&self) -> usize {
        self.users.lock().await.len()
    }

    pub async fn online(&self, user: UserId) -> Result<bool, BusError> {
        if let Some(redis) = &self.redis {
            return Ok(redis.online(user).await);
        }
        self.bucket
            .alive(&user.to_string())
            .await
            .map(|alive| alive > 0)
    }

    pub async fn join(
        self: &Arc<Realtime>,
        principal: &Principal,
        outbox: Outbox,
        after: Option<u64>,
    ) -> Seat {
        if let Some(after) = after {
            outbox.hold(after);
        }
        let user = principal.id;
        let registration = self.registry.enter(user, &principal.session, outbox);
        {
            let mut users = self.users.lock().await;
            match users.get_mut(&user) {
                Some(subs) => subs.refs += 1,
                None => {
                    match Subs::open(&self.bus, &self.topics, user, Arc::clone(&self.registry))
                        .await
                    {
                        Ok(subs) => {
                            users.insert(user, subs);
                        }
                        Err(error) => {
                            tracing::warn!("{NAME}: подписка {user} не открылась: {error}")
                        }
                    }
                }
            }
        }
        let key = self.key(user, registration.conn());
        if let Err(error) = self.bucket.touch(&key).await {
            tracing::warn!("{NAME}: присутствие {user} не записано: {error}");
        }
        if let Some(redis) = &self.redis {
            redis.touch(user).await;
        }
        Seat {
            realtime: Arc::clone(self),
            registration,
        }
    }

    pub async fn catch_up(&self, user: UserId, outbox: &Outbox, after: u64) -> bool {
        let words = self.topics.chat(&user.to_string());
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        let stream = self.topics.chat_stream();
        let mut from = after;
        if let Ok(subject) = Subject::new(&words) {
            loop {
                let page =
                    history::<DomainEvent>(&self.bus, &stream, &subject, from, CATCH_UP_PAGE);
                let page = match page.await {
                    Ok(page) => page,
                    Err(error) => {
                        tracing::warn!(
                            "{NAME}: история {user} после №{from} не прочитана: {error}"
                        );
                        break;
                    }
                };
                let full = page.len() == CATCH_UP_PAGE;
                for stored in page {
                    from = stored.sequence;
                    let Some(push) = message(user, &stored) else {
                        continue;
                    };
                    let Ok(text) = contracts::encode(&push) else {
                        continue;
                    };
                    if !outbox.caught(stored.sequence, text).await {
                        return false;
                    }
                }
                if !full {
                    break;
                }
            }
        }
        outbox.release().await
    }

    async fn leave(&self, user: UserId, conn: ConnId) {
        if let Err(error) = self.bucket.forget(&self.key(user, conn)).await {
            tracing::warn!("{NAME}: присутствие {user} не снято, уйдёт по сроку: {error}");
        }
        let mut users = self.users.lock().await;
        let last = match users.get_mut(&user) {
            Some(subs) => {
                subs.refs -= 1;
                subs.refs == 0
            }
            None => false,
        };
        if last {
            if let Some(subs) = users.remove(&user) {
                subs.close().await;
            }
            if let Some(redis) = &self.redis {
                redis.release(user).await;
            }
        }
    }

    async fn listen(
        self: Arc<Realtime>,
        mut revocations: Subscription<SessionRevoked>,
        closing: CancellationToken,
    ) {
        loop {
            let item = tokio::select! {
                () = closing.cancelled() => break,
                item = revocations.next() => item,
            };
            match item {
                Some(Ok(envelope)) => {
                    let fresh = self.revoked.remember(&envelope.body.sessions);
                    self.registry.revoke(&fresh);
                }
                Some(Err(error)) => tracing::warn!("{NAME}: отзыв сессии не разобран: {error}"),
                None => return,
            }
        }
        revocations.stop().await;
    }

    async fn beat(self: Arc<Realtime>, every: Duration, closing: CancellationToken) {
        let mut ticker = tokio::time::interval(every.max(Duration::from_millis(10)));
        loop {
            tokio::select! {
                () = closing.cancelled() => return,
                _ = ticker.tick() => {}
            }
            for (user, conn) in self.registry.seats() {
                if let Some(redis) = &self.redis {
                    redis.touch(user).await;
                }
                if let Err(error) = self.bucket.touch(&self.key(user, conn)).await {
                    tracing::warn!("{NAME}: сигнал присутствия {user} не ушёл: {error}");
                    break;
                }
            }
        }
    }
}

impl Seat {
    pub fn user(&self) -> UserId {
        self.registration.user()
    }
}

impl Drop for Seat {
    fn drop(&mut self) {
        let realtime = Arc::clone(&self.realtime);
        let (user, conn) = (self.registration.user(), self.registration.conn());
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move { realtime.leave(user, conn).await });
        }
    }
}
