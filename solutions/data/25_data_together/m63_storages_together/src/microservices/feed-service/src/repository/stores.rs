use std::fmt;
use std::sync::Arc;

use super::{FeedRepository, MemoryFeed};
use crate::common::FeedLimits;

#[derive(Clone)]
pub struct Stores {
    pub feed: Arc<dyn FeedRepository>,
}

impl Stores {
    pub fn memory(limits: FeedLimits) -> Stores {
        Stores {
            feed: Arc::new(MemoryFeed::with_limits(limits)),
        }
    }
}

impl fmt::Debug for Stores {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Stores")
    }
}

impl PartialEq for Stores {
    fn eq(&self, other: &Stores) -> bool {
        std::ptr::addr_eq(Arc::as_ptr(&self.feed), Arc::as_ptr(&other.feed))
    }
}

impl Eq for Stores {}
