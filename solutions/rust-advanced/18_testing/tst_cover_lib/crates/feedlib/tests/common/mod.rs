use feedlib::{Clock, Feed, VecStore};
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone)]
pub struct FakeClock(Rc<Cell<u64>>);

impl FakeClock {
    pub fn advance(&self, secs: u64) {
        self.0.set(self.0.get() + secs);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> u64 {
        self.0.get()
    }
}

pub fn feed_at(start: u64) -> (Feed<FakeClock, VecStore>, FakeClock) {
    let clock = FakeClock(Rc::new(Cell::new(start)));
    (Feed::new(clock.clone(), VecStore::default()), clock)
}
