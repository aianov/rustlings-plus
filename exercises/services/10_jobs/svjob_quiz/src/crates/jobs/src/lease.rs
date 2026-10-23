use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Lease {
    pub name: String,
    pub owner: String,
    pub term: u64,
}

struct Held {
    owner: String,
    term: u64,
    expires: u64,
}

pub struct LeaseStore {
    ttl: u64,
    held: HashMap<String, Held>,
    last_term: u64,
}

impl LeaseStore {
    pub fn new(ttl: u64) -> LeaseStore {
        LeaseStore { ttl, held: HashMap::new(), last_term: 0 }
    }

    pub fn acquire(&mut self, name: &str, owner: &str, now: u64) -> Option<Lease> {
        let _ = (name, owner, now);
        todo!()
    }

    pub fn renew(&mut self, lease: &Lease, now: u64) -> bool {
        let _ = (lease, now);
        todo!()
    }

    pub fn release(&mut self, lease: &Lease) {
        let _ = (lease,);
        todo!()
    }
}
