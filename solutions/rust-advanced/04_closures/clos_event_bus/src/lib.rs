pub struct Event {
    pub topic: String,
    pub data: String,
}

pub type SubId = u64;

type Handler = Box<dyn FnMut(&Event) + Send + 'static>;

struct Subscription {
    id: SubId,
    topic: String,
    handler: Handler,
}

pub struct EventBus {
    next_id: SubId,
    subscriptions: Vec<Subscription>,
}

impl Default for EventBus {
    fn default() -> Self {
        EventBus::new()
    }
}

impl EventBus {
    pub fn new() -> EventBus {
        EventBus { next_id: 1, subscriptions: Vec::new() }
    }

    pub fn subscribe(&mut self, topic: &str, handler: impl FnMut(&Event) + Send + 'static) -> SubId {
        let id = self.next_id;
        self.next_id += 1;
        self.subscriptions.push(Subscription { id, topic: topic.to_string(), handler: Box::new(handler) });
        id
    }

    pub fn unsubscribe(&mut self, id: SubId) -> bool {
        let before = self.subscriptions.len();
        self.subscriptions.retain(|s| s.id != id);
        self.subscriptions.len() != before
    }

    pub fn publish(&mut self, topic: &str, data: &str) -> usize {
        let event = Event { topic: topic.to_string(), data: data.to_string() };
        let mut called = 0;
        for subscription in self.subscriptions.iter_mut().filter(|s| s.topic == topic || s.topic == "*") {
            (subscription.handler)(&event);
            called += 1;
        }
        called
    }

    pub fn subscribers(&self, topic: &str) -> usize {
        self.subscriptions.iter().filter(|s| s.topic == topic || s.topic == "*").count()
    }
}
