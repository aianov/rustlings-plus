use crate::domain::{PostId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    NewPost {
        author: UserId,
        post: PostId,
    },
    Liked {
        by: UserId,
        post: PostId,
        author: UserId,
    },
    Followed {
        who: UserId,
        whom: UserId,
    },
    MessageSent {
        from: UserId,
        to: UserId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Topic {
    NewPost,
    Liked,
    Followed,
    MessageSent,
    All,
}

impl Event {
    pub fn topic(&self) -> Topic {
        match self {
            Event::NewPost { .. } => Topic::NewPost,
            Event::Liked { .. } => Topic::Liked,
            Event::Followed { .. } => Topic::Followed,
            Event::MessageSent { .. } => Topic::MessageSent,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SubId(pub u64);

type Handler = Box<dyn FnMut(&Event) + Send + 'static>;

struct Subscription {
    id: SubId,
    topic: Topic,
    handler: Handler,
}

#[derive(Default)]
pub struct EventBus {
    last_id: u64,
    subscriptions: Vec<Subscription>,
}

impl EventBus {
    pub fn new() -> EventBus {
        EventBus::default()
    }

    pub fn subscribe(
        &mut self,
        topic: Topic,
        handler: impl FnMut(&Event) + Send + 'static,
    ) -> SubId {
        self.last_id += 1;
        let id = SubId(self.last_id);
        self.subscriptions.push(Subscription {
            id,
            topic,
            handler: Box::new(handler),
        });
        id
    }

    pub fn unsubscribe(&mut self, id: SubId) -> bool {
        let before = self.subscriptions.len();
        self.subscriptions
            .retain(|subscription| subscription.id != id);
        self.subscriptions.len() < before
    }

    pub fn publish(&mut self, event: &Event) -> usize {
        let topic = event.topic();
        let mut called = 0;
        for subscription in self
            .subscriptions
            .iter_mut()
            .filter(|subscription| hears(subscription.topic, topic))
        {
            (subscription.handler)(event);
            called += 1;
        }
        called
    }

    pub fn subscribers(&self, topic: Topic) -> usize {
        self.subscriptions
            .iter()
            .filter(|subscription| hears(subscription.topic, topic))
            .count()
    }
}

fn hears(subscribed: Topic, published: Topic) -> bool {
    subscribed == published || subscribed == Topic::All
}
