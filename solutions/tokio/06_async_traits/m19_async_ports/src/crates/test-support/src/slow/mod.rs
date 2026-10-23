mod chats;
mod posts;
mod users;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use social_core::{MemRepo, Network, PostStore, Repos, UserStore};

use chats::SlowChats;
use posts::SlowPosts;
use users::SlowUsers;

#[derive(Debug)]
struct Pace {
    delay: Duration,
    calls: AtomicUsize,
}

impl Pace {
    async fn wait(&self) {
        self.calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.delay).await;
    }
}

#[derive(Clone)]
pub struct SlowRepos {
    repos: Repos,
    pace: Arc<Pace>,
}

impl SlowRepos {
    pub fn wrap(inner: Repos, delay: Duration) -> SlowRepos {
        let pace = Arc::new(Pace {
            delay,
            calls: AtomicUsize::new(0),
        });
        let repos = Repos {
            users: Arc::new(SlowUsers::new(inner.users, Arc::clone(&pace))),
            posts: Arc::new(SlowPosts::new(inner.posts, Arc::clone(&pace))),
            chats: Arc::new(SlowChats::new(inner.chats, Arc::clone(&pace))),
        };
        SlowRepos { repos, pace }
    }

    pub fn over<U, P>(network: Network<U, P>, delay: Duration) -> SlowRepos
    where
        U: UserStore + Send + 'static,
        P: PostStore + Send + 'static,
    {
        SlowRepos::wrap(MemRepo::repos(network), delay)
    }

    pub fn repos(&self) -> Repos {
        self.repos.clone()
    }

    pub fn delay(&self) -> Duration {
        self.pace.delay
    }

    pub fn calls(&self) -> usize {
        self.pace.calls.load(Ordering::SeqCst)
    }
}
