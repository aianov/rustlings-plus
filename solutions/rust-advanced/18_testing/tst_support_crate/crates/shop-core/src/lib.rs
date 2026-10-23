use contracts::{Clock, Ids};

#[derive(Debug, Clone, PartialEq)]
pub struct Order {
    pub id: u64,
    pub item: String,
    pub placed_at: u64,
}

pub struct Orders<C: Clock, I: Ids> {
    clock: C,
    ids: I,
    placed: std::sync::Mutex<Vec<Order>>,
}

impl<C: Clock, I: Ids> Orders<C, I> {
    pub fn new(clock: C, ids: I) -> Self {
        Orders { clock, ids, placed: std::sync::Mutex::new(Vec::new()) }
    }

    pub fn place(&self, item: &str) -> Order {
        let order = Order { id: self.ids.next_id(), item: item.to_string(), placed_at: self.clock.now() };
        self.placed.lock().expect("замок заказов отравлен").push(order.clone());
        order
    }

    pub fn older_than(&self, secs: u64) -> Vec<u64> {
        let now = self.clock.now();
        let placed = self.placed.lock().expect("замок заказов отравлен");
        placed.iter().filter(|order| now.saturating_sub(order.placed_at) > secs).map(|order| order.id).collect()
    }
}
