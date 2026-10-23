use social_core::{MemNetwork, UserId};
use test_support::FakeClock;

pub const START: u64 = 1_700_000_000;

pub fn network() -> (MemNetwork, FakeClock) {
    let clock = FakeClock::new(START);
    let network = MemNetwork::new().with_clock(clock.clone());
    (network, clock)
}

pub fn register(network: &mut MemNetwork, name: &str) -> UserId {
    network
        .register(name)
        .unwrap_or_else(|e| panic!("{name} должен регистрироваться: {e}"))
}
