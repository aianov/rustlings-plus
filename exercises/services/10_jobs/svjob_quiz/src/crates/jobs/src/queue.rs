#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub id: String,
    pub kind: String,
    pub run_at: u64,
}

impl Job {
    pub fn new(id: &str, kind: &str, run_at: u64) -> Job {
        Job { id: id.to_string(), kind: kind.to_string(), run_at }
    }
}

#[derive(Debug, Clone)]
pub struct Delivery {
    pub job: Job,
    pub delivered: u32,
}

struct Slot {
    job: Job,
    delivered: u32,
    ready_at: u64,
    leased_until: u64,
}

pub struct MemQueue {
    ack_wait: u64,
    slots: Vec<Slot>,
    dead: Vec<(Job, String)>,
}

impl MemQueue {
    pub fn new(ack_wait: u64) -> MemQueue {
        MemQueue { ack_wait, slots: Vec::new(), dead: Vec::new() }
    }

    pub fn enqueue(&mut self, job: Job) {
        if self.slots.iter().any(|s| s.job.id == job.id) {
            return;
        }
        self.slots.push(Slot { job, delivered: 0, ready_at: 0, leased_until: 0 });
    }

    pub fn take(&mut self, now: u64) -> Option<Delivery> {
        let slot = self.slots.iter_mut().find(|s| s.ready_at <= now && s.leased_until <= now)?;
        slot.delivered += 1;
        slot.leased_until = now + self.ack_wait;
        Some(Delivery { job: slot.job.clone(), delivered: slot.delivered })
    }

    pub fn ack(&mut self, id: &str) {
        self.slots.retain(|s| s.job.id != id);
    }

    pub fn nak(&mut self, id: &str, delay: u64, now: u64) {
        if let Some(slot) = self.slots.iter_mut().find(|s| s.job.id == id) {
            slot.ready_at = now + delay;
            slot.leased_until = 0;
        }
    }

    pub fn term(&mut self, id: &str, reason: &str) {
        if let Some(i) = self.slots.iter().position(|s| s.job.id == id) {
            let slot = self.slots.remove(i);
            self.dead.push((slot.job, reason.to_string()));
        }
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn dead(&self) -> &[(Job, String)] {
        &self.dead
    }
}
