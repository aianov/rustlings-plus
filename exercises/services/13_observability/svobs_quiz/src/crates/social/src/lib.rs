mod bus;
mod gateway;
mod notify;
mod posts;

pub use bus::{Bus, Headers, Message};
pub use gateway::{Gateway, Request, Response};
pub use notify::NotifyService;
pub use posts::{PostError, PostService};

use std::sync::Arc;

pub type Ids = Arc<dyn Fn() -> u64 + Send + Sync>;

pub struct Stack {
    pub gateway: Gateway,
    pub posts: Arc<PostService>,
    pub notify: NotifyService,
    pub bus: Bus,
}

impl Stack {
    pub fn new(ids: Ids) -> Stack {
        let bus = Bus::default();
        let posts = Arc::new(PostService::new(bus.clone(), ids.clone()));
        let notify = NotifyService::new(bus.clone(), ids.clone());
        let gateway = Gateway::new(posts.clone(), ids);
        Stack { gateway, posts, notify, bus }
    }
}
