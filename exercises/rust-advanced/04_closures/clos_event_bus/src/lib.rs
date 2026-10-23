pub struct Event {
    pub topic: String,
    pub data: String,
}

pub type SubId = u64;

pub struct EventBus {
    next_id: SubId,
}

impl EventBus {
    pub fn new() -> EventBus {
        EventBus { next_id: 1 }
    }

    pub fn subscribe(&mut self, topic: &str, handler: impl FnMut(&Event)) -> SubId {
        todo!("запомнить обработчик, вернуть id подписки")
    }

    pub fn unsubscribe(&mut self, id: SubId) -> bool {
        todo!("убрать подписку; true, если такая была")
    }

    pub fn publish(&mut self, topic: &str, data: &str) -> usize {
        todo!("вызвать подходящих подписчиков; вернуть их число")
    }

    pub fn subscribers(&self, topic: &str) -> usize {
        todo!("сколько обработчиков получит событие этой темы")
    }
}
