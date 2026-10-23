use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq)]
pub struct Delivery {
    pub seq: u64,
    pub subject: String,
    pub text: String,
}

#[derive(Default)]
struct Inner {
    next_sub: u64,
    subs: HashMap<String, Vec<(u64, Sender<Delivery>)>>,
    log: Vec<Delivery>,
    hook: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Inner {
    fn deliver(&self, delivery: &Delivery) {
        for (_, tx) in self.subs.get(&delivery.subject).into_iter().flatten() {
            tx.send(delivery.clone()).ok();
        }
    }
}

#[derive(Clone, Default)]
pub struct Bus {
    inner: Arc<Mutex<Inner>>,
}

pub struct Subscription {
    id: u64,
    subject: String,
    rx: Receiver<Delivery>,
    bus: Bus,
}

impl Subscription {
    pub fn try_next(&self) -> Option<Delivery> {
        self.rx.try_recv().ok()
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let mut inner = self.bus.inner.lock().unwrap();
        if let Some(list) = inner.subs.get_mut(&self.subject) {
            list.retain(|(id, _)| *id != self.id);
            if list.is_empty() {
                inner.subs.remove(&self.subject);
            }
        }
    }
}

impl Bus {
    pub fn new() -> Bus {
        Bus::default()
    }

    pub fn subscribe(&self, subject: &str) -> Subscription {
        let (tx, rx) = channel();
        let mut inner = self.inner.lock().unwrap();
        inner.next_sub += 1;
        let id = inner.next_sub;
        inner.subs.entry(subject.to_string()).or_default().push((id, tx));
        Subscription { id, subject: subject.to_string(), rx, bus: self.clone() }
    }

    pub fn publish(&self, subject: &str, text: &str) {
        let inner = self.inner.lock().unwrap();
        inner.deliver(&Delivery { seq: 0, subject: subject.to_string(), text: text.to_string() });
    }

    pub fn append(&self, subject: &str, text: &str) -> u64 {
        let mut inner = self.inner.lock().unwrap();
        let seq = inner.log.len() as u64 + 1;
        let delivery = Delivery { seq, subject: subject.to_string(), text: text.to_string() };
        inner.log.push(delivery.clone());
        inner.deliver(&delivery);
        seq
    }

    pub fn history_after(&self, subject: &str, seq: u64) -> Vec<Delivery> {
        let (snapshot, hook) = {
            let inner = self.inner.lock().unwrap();
            let snapshot: Vec<Delivery> = inner.log.iter().filter(|d| d.subject == subject && d.seq > seq).cloned().collect();
            (snapshot, inner.hook.clone())
        };
        if let Some(hook) = hook {
            hook();
        }
        snapshot
    }

    pub fn set_history_hook(&self, hook: impl Fn() + Send + Sync + 'static) {
        self.inner.lock().unwrap().hook = Some(Arc::new(hook));
    }

    pub fn clear_history_hook(&self) {
        self.inner.lock().unwrap().hook = None;
    }

    pub fn subscribers(&self, subject: &str) -> usize {
        self.inner.lock().unwrap().subs.get(subject).map_or(0, Vec::len)
    }

    pub fn stored(&self) -> usize {
        self.inner.lock().unwrap().log.len()
    }
}
