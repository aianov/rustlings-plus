use std::fmt;
use std::sync::Arc;

use super::{MemoryNotices, NoticeRepository};

#[derive(Clone)]
pub struct Stores {
    pub notices: Arc<dyn NoticeRepository>,
}

impl Stores {
    pub fn memory() -> Stores {
        Stores {
            notices: Arc::new(MemoryNotices::new()),
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
        std::ptr::addr_eq(Arc::as_ptr(&self.notices), Arc::as_ptr(&other.notices))
    }
}

impl Eq for Stores {}
