pub trait Clock {
    fn now(&self) -> u64;
}

pub trait Ids {
    fn next_id(&self) -> u64;
}
