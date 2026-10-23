use std::fmt;
use std::sync::Arc;

use super::{
    DraftRepository, FullText, MemoryPosts, OutboxRepository, PgPosts, PinQueries, PostQueries,
    PostRepository, SearchQueries, ThreadQueries,
};

#[derive(Clone)]
pub struct Extras {
    pub threads: Arc<dyn ThreadQueries>,
    pub pins: Arc<dyn PinQueries>,
    pub search: Arc<dyn SearchQueries>,
    pub fulltext: Arc<dyn FullText>,
}

#[derive(Clone)]
pub struct Stores {
    pub posts: Arc<dyn PostRepository>,
    pub drafts: Arc<dyn DraftRepository>,
    pub outbox: Arc<dyn OutboxRepository>,
    pub queries: Arc<dyn PostQueries>,
    pub extras: Option<Extras>,
}

impl Stores {
    pub fn memory() -> Stores {
        let posts = Arc::new(MemoryPosts::new());
        Stores {
            posts: posts.clone(),
            drafts: posts.clone(),
            outbox: posts.clone(),
            queries: posts,
            extras: None,
        }
    }

    pub fn postgres(posts: Arc<PgPosts>) -> Stores {
        Stores {
            posts: posts.clone(),
            drafts: posts.clone(),
            outbox: posts.clone(),
            queries: posts.clone(),
            extras: Some(Extras {
                threads: posts.clone(),
                pins: posts.clone(),
                search: posts.clone(),
                fulltext: posts,
            }),
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
        std::ptr::addr_eq(Arc::as_ptr(&self.outbox), Arc::as_ptr(&other.outbox))
    }
}

impl Eq for Stores {}
