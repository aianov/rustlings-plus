use tokio::sync::mpsc::UnboundedReceiver;

fn main() {}

#[derive(Debug, Clone, PartialEq)]
pub struct Msg {
    pub seq: u64,
    pub text: String,
}

pub trait Backend {
    fn subscribe(&self) -> UnboundedReceiver<Msg>;
    fn history_after(&self, seq: u64) -> Vec<Msg>;
}

pub struct Tail {
    rx: UnboundedReceiver<Msg>,
    last: u64,
}

impl Tail {
    pub fn new(rx: UnboundedReceiver<Msg>, last: u64) -> Tail {
        Tail { rx, last }
    }

    pub fn next_ready(&mut self) -> Option<Msg> {
        while let Ok(message) = self.rx.try_recv() {
            if message.seq > self.last {
                self.last = message.seq;
                return Some(message);
            }
        }
        None
    }
}

pub struct Resumed {
    pub backlog: Vec<Msg>,
    pub tail: Tail,
}

pub fn resume(backend: &dyn Backend, last_seen: u64) -> Resumed {
    let rx = backend.subscribe();
    let mut backlog = backend.history_after(last_seen);
    backlog.sort_by_key(|m| m.seq);
    backlog.dedup_by_key(|m| m.seq);
    let mut tail = Tail::new(rx, backlog.last().map_or(last_seen, |m| m.seq));
    while let Some(message) = tail.next_ready() {
        backlog.push(message);
    }
    Resumed { backlog, tail }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

    fn msg(seq: u64) -> Msg {
        Msg { seq, text: format!("m{seq}") }
    }

    #[derive(Default)]
    struct Inner {
        store: Vec<Msg>,
        subs: Vec<UnboundedSender<Msg>>,
        before_subscribe: Vec<Msg>,
        after_subscribe: Vec<Msg>,
        during_history: Vec<Msg>,
    }

    impl Inner {
        fn publish(&mut self, message: Msg) {
            self.store.push(message.clone());
            for sub in &self.subs {
                sub.send(message.clone()).ok();
            }
        }
    }

    #[derive(Default)]
    struct Fake {
        inner: Mutex<Inner>,
    }

    impl Fake {
        fn with(stored: &[u64]) -> Fake {
            let fake = Fake::default();
            fake.inner.lock().unwrap().store = stored.iter().map(|s| msg(*s)).collect();
            fake
        }

        fn script(&self, before_subscribe: &[u64], after_subscribe: &[u64], during_history: &[u64]) {
            let mut inner = self.inner.lock().unwrap();
            inner.before_subscribe = before_subscribe.iter().map(|s| msg(*s)).collect();
            inner.after_subscribe = after_subscribe.iter().map(|s| msg(*s)).collect();
            inner.during_history = during_history.iter().map(|s| msg(*s)).collect();
        }

        fn publish(&self, message: Msg) {
            self.inner.lock().unwrap().publish(message);
        }
    }

    impl Backend for Fake {
        fn subscribe(&self) -> UnboundedReceiver<Msg> {
            let mut inner = self.inner.lock().unwrap();
            let early = std::mem::take(&mut inner.before_subscribe);
            inner.store.extend(early);
            let (tx, rx) = unbounded_channel();
            inner.subs.push(tx);
            for message in std::mem::take(&mut inner.after_subscribe) {
                inner.publish(message);
            }
            rx
        }

        fn history_after(&self, seq: u64) -> Vec<Msg> {
            let mut inner = self.inner.lock().unwrap();
            let snapshot = inner.store.iter().filter(|m| m.seq > seq).cloned().collect();
            for message in std::mem::take(&mut inner.during_history) {
                inner.publish(message);
            }
            snapshot
        }
    }

    fn seqs(messages: &[Msg]) -> Vec<u64> {
        messages.iter().map(|m| m.seq).collect()
    }

    #[test]
    fn a_quiet_chat_returns_exactly_what_was_missed() {
        let fake = Fake::with(&[1, 2, 3, 4, 5]);
        let mut resumed = resume(&fake, 2);
        assert_eq!(seqs(&resumed.backlog), vec![3, 4, 5], "клиент видел до второго");
        assert!(resumed.tail.next_ready().is_none(), "живых пока нет");
        assert!(resume(&fake, 5).backlog.is_empty(), "всё уже видел");
    }

    #[test]
    fn a_message_that_arrives_while_the_history_is_being_read_is_not_lost() {
        let fake = Fake::with(&[1, 2, 3]);
        fake.script(&[], &[], &[4]);
        let resumed = resume(&fake, 1);
        assert_eq!(seqs(&resumed.backlog), vec![2, 3, 4], "сообщение 4 пришло во время чтения истории: подписка должна уже стоять");
    }

    #[test]
    fn a_message_published_before_the_subscription_comes_from_the_history() {
        let fake = Fake::with(&[1, 2, 3]);
        fake.script(&[4], &[], &[]);
        assert_eq!(seqs(&resume(&fake, 0).backlog), vec![1, 2, 3, 4], "сообщение, появившееся до подписки, видно в истории, которую читаем после подписки");
    }

    #[test]
    fn a_message_seen_twice_is_delivered_once() {
        let fake = Fake::with(&[1, 2, 3]);
        fake.script(&[], &[4], &[]);
        let mut resumed = resume(&fake, 0);
        assert_eq!(seqs(&resumed.backlog), vec![1, 2, 3, 4], "сообщение 4 есть и в истории, и в живом потоке, но один раз");
        assert!(resumed.tail.next_ready().is_none(), "живая копия уже учтена и в хвост не попадает");
    }

    #[test]
    fn all_three_gaps_at_once_give_an_ordered_list_without_holes_or_repeats() {
        let fake = Fake::with(&[1, 2, 3]);
        fake.script(&[4], &[5], &[6]);
        let resumed = resume(&fake, 2);
        assert_eq!(seqs(&resumed.backlog), vec![3, 4, 5, 6], "по порядку, каждое один раз");
    }

    #[test]
    fn the_tail_continues_without_repeating_what_the_backlog_has() {
        let fake = Fake::with(&[1, 2]);
        fake.script(&[], &[3], &[]);
        let mut resumed = resume(&fake, 0);
        fake.publish(msg(3));
        fake.publish(msg(4));
        fake.publish(msg(2));
        fake.publish(msg(5));
        assert_eq!(resumed.tail.next_ready().map(|m| m.seq), Some(4), "повторы 3 и 2 пропущены, новое 4 пришло");
        assert_eq!(resumed.tail.next_ready().map(|m| m.seq), Some(5), "потом 5");
        assert!(resumed.tail.next_ready().is_none(), "больше ничего");
    }
}
