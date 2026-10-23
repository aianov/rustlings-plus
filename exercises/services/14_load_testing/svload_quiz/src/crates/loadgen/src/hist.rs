use std::time::Duration;

pub struct LogHistogram {
    // TODO: что здесь хранится, решаешь ты.
}

impl LogHistogram {
    pub fn new(bits: u32) -> LogHistogram {
        let _ = bits;
        todo!()
    }

    pub fn record(&mut self, value: Duration) {
        let _ = value;
        todo!()
    }

    pub fn count(&self) -> u64 {
        todo!()
    }

    pub fn min(&self) -> Duration {
        todo!()
    }

    pub fn max(&self) -> Duration {
        todo!()
    }

    pub fn buckets_len(&self) -> usize {
        todo!()
    }

    pub fn value_at_quantile(&self, q: f64) -> Duration {
        let _ = q;
        todo!()
    }

    pub fn merge(&mut self, other: &LogHistogram) {
        let _ = other;
        todo!()
    }
}
