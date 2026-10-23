use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use contracts::{Push, UserId};
use grpc_kit::Stamp;
use proto::social::v1::SubscribeRequest;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};

use crate::client::Upstreams;
use crate::common::Registry;

pub const OPEN_WAIT: Duration = Duration::from_secs(1);

const FIRST_RETRY: Duration = Duration::from_millis(50);

const LAST_RETRY: Duration = Duration::from_secs(2);

struct Open {
    devices: usize,
    task: JoinHandle<()>,
}

pub struct Inboxes {
    upstreams: Upstreams,
    registry: Arc<Registry>,
    open: Mutex<HashMap<UserId, Open>>,
}

pub struct InboxGuard {
    inboxes: Arc<Inboxes>,
    user: UserId,
}

async fn listen(
    upstreams: Upstreams,
    registry: Arc<Registry>,
    user: UserId,
    mut opened: Option<oneshot::Sender<()>>,
) {
    let mut pause = FIRST_RETRY;
    loop {
        let stamp = Stamp::fresh().user(user.0);
        let subscribed = upstreams.chat(&stamp).subscribe(SubscribeRequest {}).await;
        if let Some(opened) = opened.take() {
            let _ = opened.send(());
        }
        match subscribed {
            Ok(stream) => {
                pause = FIRST_RETRY;
                let mut stream = stream.into_inner();
                while let Ok(Some(message)) = stream.message().await {
                    let (to, from) = (UserId(message.to_id), UserId(message.from_id));
                    registry.deliver(to, &Push::Message { to, from });
                }
            }
            Err(status) => eprintln!(
                "api-gateway [{}]: подписка {user} на сообщения не открылась: {status}",
                stamp.request_id()
            ),
        }
        sleep(pause).await;
        pause = (pause * 2).min(LAST_RETRY);
    }
}

impl Inboxes {
    pub fn new(upstreams: Upstreams, registry: Arc<Registry>) -> Inboxes {
        Inboxes {
            upstreams,
            registry,
            open: Mutex::default(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<UserId, Open>> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub async fn join(self: &Arc<Inboxes>, user: UserId) -> InboxGuard {
        let opened = {
            let mut open = self.lock();
            match open.get_mut(&user) {
                Some(entry) => {
                    entry.devices += 1;
                    None
                }
                None => {
                    let (tx, rx) = oneshot::channel();
                    let upstreams = self.upstreams.clone();
                    let registry = Arc::clone(&self.registry);
                    let task = tokio::spawn(listen(upstreams, registry, user, Some(tx)));
                    open.insert(user, Open { devices: 1, task });
                    Some(rx)
                }
            }
        };
        let guard = InboxGuard {
            inboxes: Arc::clone(self),
            user,
        };
        if let Some(opened) = opened {
            let _ = timeout(OPEN_WAIT, opened).await;
        }
        guard
    }

    pub fn open(&self) -> usize {
        self.lock().len()
    }

    fn leave(&self, user: UserId) {
        let mut open = self.lock();
        let last = match open.get_mut(&user) {
            Some(entry) => {
                entry.devices -= 1;
                entry.devices == 0
            }
            None => false,
        };
        if last {
            if let Some(entry) = open.remove(&user) {
                entry.task.abort();
            }
        }
    }
}

impl Drop for InboxGuard {
    fn drop(&mut self) {
        self.inboxes.leave(self.user);
    }
}
