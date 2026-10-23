#[derive(Debug, PartialEq)]
pub enum Upload {
    Pending(Vec<u8>),
    Hashed(u64),
    Failed(String),
}

fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(7u64, |acc, b| acc.wrapping_mul(31).wrapping_add(u64::from(*b)))
}

impl Upload {
    pub fn new(bytes: Vec<u8>) -> Upload {
        Upload::Pending(bytes)
    }

    pub fn fail(&mut self, why: &str) {
        if let Upload::Pending(_) = self {
            *self = Upload::Failed(why.to_string());
        }
    }

    pub fn finish(&mut self) {
        match std::mem::replace(self, Upload::Hashed(0)) {
            Upload::Pending(bytes) => *self = Upload::Hashed(checksum(&bytes)),
            other => *self = other,
        }
    }

    pub fn take_bytes(&mut self) -> Option<Vec<u8>> {
        match std::mem::replace(self, Upload::Hashed(0)) {
            Upload::Pending(bytes) => Some(bytes),
            other => {
                *self = other;
                None
            }
        }
    }
}

fn main() {
    let mut u = Upload::new(vec![1, 2, 3]);
    u.finish();
    println!("{u:?}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_hashes_pending_bytes() {
        let mut u = Upload::new(vec![1, 2, 3]);
        u.finish();
        assert_eq!(u, Upload::Hashed(checksum(&[1, 2, 3])));
    }

    #[test]
    fn finish_does_not_touch_other_states() {
        let mut done = Upload::Hashed(5);
        done.finish();
        assert_eq!(done, Upload::Hashed(5));
        let mut failed = Upload::new(vec![1]);
        failed.fail("disk full");
        failed.finish();
        assert_eq!(failed, Upload::Failed("disk full".to_string()), "причина отказа не должна теряться при finish");
    }

    #[test]
    fn take_bytes_hands_over_the_buffer_once() {
        let mut u = Upload::new(vec![9, 8]);
        assert_eq!(u.take_bytes(), Some(vec![9, 8]));
        assert_eq!(u, Upload::Hashed(0));
        assert_eq!(u.take_bytes(), None);
    }

    #[test]
    fn take_bytes_keeps_a_failure_reason() {
        let mut u = Upload::Failed("net".to_string());
        assert_eq!(u.take_bytes(), None);
        assert_eq!(u, Upload::Failed("net".to_string()), "состояние Failed остаётся на месте");
    }
}
