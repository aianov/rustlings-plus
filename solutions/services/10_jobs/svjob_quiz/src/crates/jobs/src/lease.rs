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
        if self.held.get(name).is_some_and(|h| now < h.expires) {
            return None;
        }
        self.last_term += 1;
        self.held.insert(name.to_string(), Held { owner: owner.to_string(), term: self.last_term, expires: now + self.ttl });
        Some(Lease { name: name.to_string(), owner: owner.to_string(), term: self.last_term })
    }

    pub fn renew(&mut self, lease: &Lease, now: u64) -> bool {
        match self.held.get_mut(&lease.name) {
            Some(h) if h.owner == lease.owner && h.term == lease.term && now < h.expires => {
                h.expires = now + self.ttl;
                true
            }
            _ => false,
        }
    }

    pub fn release(&mut self, lease: &Lease) {
        if self.held.get(&lease.name).is_some_and(|h| h.owner == lease.owner && h.term == lease.term) {
            self.held.remove(&lease.name);
        }
    }
}
