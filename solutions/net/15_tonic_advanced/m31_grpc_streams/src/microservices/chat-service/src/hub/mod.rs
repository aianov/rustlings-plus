use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use proto::social::v1::Message;
use tokio::sync::mpsc;
use tonic::Status;

pub const BACKLOG: usize = 64;

type Inbox = mpsc::Sender<Result<Message, Status>>;

#[derive(Default)]
pub struct Hub {
    inboxes: Mutex<HashMap<u64, Vec<Inbox>>>,
    closed: AtomicBool,
}

impl Hub {
    fn lock(&self) -> MutexGuard<'_, HashMap<u64, Vec<Inbox>>> {
        self.inboxes.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn subscribe(&self, user: u64) -> Result<mpsc::Receiver<Result<Message, Status>>, Status> {
        let (tx, rx) = mpsc::channel(BACKLOG);
        let mut inboxes = self.lock();
        if self.closed.load(Ordering::SeqCst) {
            return Err(Status::unavailable("chat-service останавливается"));
        }
        let own = inboxes.entry(user).or_default();
        own.retain(|inbox| !inbox.is_closed());
        own.push(tx);
        Ok(rx)
    }

    pub fn close(&self) {
        let mut inboxes = self.lock();
        self.closed.store(true, Ordering::SeqCst);
        inboxes.clear();
    }

    pub fn deliver(&self, message: &Message) -> usize {
        let mut inboxes = self.lock();
        let Some(own) = inboxes.get_mut(&message.to_id) else {
            return 0;
        };
        let before = own.len();
        own.retain(|inbox| inbox.try_send(Ok(message.clone())).is_ok());
        let delivered = own.len();
        if own.is_empty() {
            inboxes.remove(&message.to_id);
        }
        if delivered < before {
            eprintln!(
                "chat-service: у пользователя {} закрыто подписок: {}, они ушли или не успевают читать",
                message.to_id,
                before - delivered
            );
        }
        delivered
    }

    pub fn subscribers(&self, user: u64) -> usize {
        self.lock().get(&user).map_or(0, |own| {
            own.iter().filter(|inbox| !inbox.is_closed()).count()
        })
    }
}
