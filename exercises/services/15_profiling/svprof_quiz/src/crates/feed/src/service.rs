use crate::render::render_page;
use crate::request::parse_request;
use crate::storage::{Post, read_page};
use crate::store::{Session, SessionStore};
use std::sync::Arc;

pub type SessionHook = Arc<dyn Fn(u64, &mut Session) + Send + Sync>;

pub struct FeedService {
    sessions: SessionStore,
    hook: SessionHook,
}

impl FeedService {
    pub fn new() -> FeedService {
        FeedService::with_hook(Arc::new(|_, _| ()))
    }

    pub fn with_hook(hook: SessionHook) -> FeedService {
        FeedService { sessions: SessionStore::new(16), hook }
    }

    pub fn sessions(&self) -> &SessionStore {
        &self.sessions
    }

    pub async fn handle(&self, body: &str, out: &mut String) -> Result<(), String> {
        let request = parse_request(body)?;
        self.sessions.with(request.user, |session| {
            session.hits += 1;
            (self.hook)(request.user, session);
        });
        let posts: Vec<Post> = read_page(request.user);
        render_page(request.user, &posts, request.limit, out);
        Ok(())
    }
}

impl Default for FeedService {
    fn default() -> Self {
        FeedService::new()
    }
}
