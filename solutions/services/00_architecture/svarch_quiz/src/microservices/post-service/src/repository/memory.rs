use super::{PostRepository, RepoError};
use crate::common::Post;
use contracts::{PostId, UserId};
use futures::future::BoxFuture;
use store_kit::Store;

#[derive(Default)]
pub struct InMemoryPosts {
    store: Store<Vec<Post>>,
}

impl InMemoryPosts {
    pub fn new(store: Store<Vec<Post>>) -> InMemoryPosts {
        InMemoryPosts { store }
    }
}

impl PostRepository for InMemoryPosts {
    fn create_if_absent(&self, author: UserId, text: String) -> BoxFuture<'_, Result<Post, RepoError>> {
        Box::pin(async move {
            self.store.with(|posts| {
                if posts.iter().any(|p| p.author == author && p.text == text) {
                    return Err(RepoError::Conflict);
                }
                let post = Post { id: PostId(posts.len() as u64 + 1), author, text };
                posts.push(post.clone());
                Ok(post)
            })
        })
    }

    fn find(&self, id: PostId) -> BoxFuture<'_, Option<Post>> {
        Box::pin(async move { self.store.with(|posts| posts.iter().find(|p| p.id == id).cloned()) })
    }

    fn by_author(&self, author: UserId) -> BoxFuture<'_, Vec<Post>> {
        Box::pin(async move {
            self.store.with(|posts| posts.iter().filter(|p| p.author == author).cloned().collect())
        })
    }
}
