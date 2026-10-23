pub struct Periodic {
    every: u64,
    next: u64,
}

fn spread(instance: &str, every: u64) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in instance.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash % every
}

impl Periodic {
    pub fn new(instance: &str, every: u64, now: u64) -> Periodic {
        Periodic { every, next: now + spread(instance, every) }
    }

    pub fn every(&self) -> u64 {
        self.every
    }

    pub fn next_at(&self) -> u64 {
        self.next
    }

    pub fn due(&mut self, now: u64) -> Vec<u64> {
        if self.next > now {
            return Vec::new();
        }
        let fired = vec![self.next];
        let missed = (now - self.next) / self.every + 1;
        self.next += missed * self.every;
        fired
    }
}
