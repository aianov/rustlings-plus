use futures::future::BoxFuture;
use std::sync::{Arc, Mutex};

#[derive(Debug, PartialEq)]
pub enum DirectoryError {
    Unavailable,
}

pub trait UserDirectory: Send + Sync {
    fn exists(&self, id: u64) -> BoxFuture<'_, Result<bool, DirectoryError>>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct Post {
    pub id: u64,
    pub author: u64,
    pub text: String,
}

#[derive(Debug, PartialEq)]
pub enum PostError {
    Empty,
    AuthorNotFound,
    AuthorCheckUnavailable,
}

pub struct PostService {
    users: Arc<dyn UserDirectory>,
    posts: Mutex<Vec<Post>>,
}

impl PostService {
    pub fn new(users: Arc<dyn UserDirectory>) -> Self {
        Self { users, posts: Mutex::new(Vec::new()) }
    }

    pub async fn create(&self, author: u64, text: &str) -> Result<Post, PostError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(PostError::Empty);
        }
        match self.users.exists(author).await {
            Ok(true) => {}
            Ok(false) => return Err(PostError::AuthorNotFound),
            Err(DirectoryError::Unavailable) => return Err(PostError::AuthorCheckUnavailable),
        }
        let mut posts = self.posts.lock().unwrap();
        let post = Post { id: posts.len() as u64 + 1, author, text: text.to_string() };
        posts.push(post.clone());
        Ok(post)
    }

    pub fn post_count(&self) -> usize {
        self.posts.lock().unwrap().len()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeDirectory {
        known: Vec<u64>,
        down: bool,
        asked: AtomicUsize,
    }

    impl FakeDirectory {
        fn with(known: &[u64]) -> Arc<Self> {
            Arc::new(Self { known: known.to_vec(), down: false, asked: AtomicUsize::new(0) })
        }

        fn broken() -> Arc<Self> {
            Arc::new(Self { known: vec![], down: true, asked: AtomicUsize::new(0) })
        }
    }

    impl UserDirectory for FakeDirectory {
        fn exists(&self, id: u64) -> BoxFuture<'_, Result<bool, DirectoryError>> {
            self.asked.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                if self.down {
                    Err(DirectoryError::Unavailable)
                } else {
                    Ok(self.known.contains(&id))
                }
            })
        }
    }

    #[tokio::test]
    async fn known_author_can_post() {
        let service = PostService::new(FakeDirectory::with(&[1]));
        let post = service.create(1, " привет ").await.unwrap();
        assert_eq!(post.text, "привет", "текст обрезан");
        assert_eq!(service.post_count(), 1, "пост сохранён");
    }

    #[tokio::test]
    async fn unknown_author_is_not_found() {
        let service = PostService::new(FakeDirectory::with(&[1]));
        assert_eq!(
            service.create(2, "hi").await,
            Err(PostError::AuthorNotFound),
            "сосед ответил «нет такого»: это AuthorNotFound"
        );
        assert_eq!(service.post_count(), 0, "пост от несуществующего автора не сохраняется");
    }

    #[tokio::test]
    async fn broken_neighbour_is_not_the_same_as_unknown_author() {
        let service = PostService::new(FakeDirectory::broken());
        assert_eq!(
            service.create(1, "hi").await,
            Err(PostError::AuthorCheckUnavailable),
            "сосед не ответил: это не «автора нет», иначе при его падении все пользователи «исчезнут»"
        );
        assert_eq!(service.post_count(), 0, "пока не знаем, есть ли автор, пост не сохраняем");
    }

    #[tokio::test]
    async fn empty_text_does_not_bother_the_neighbour() {
        let directory = FakeDirectory::with(&[1]);
        let service = PostService::new(directory.clone());
        assert_eq!(service.create(1, "   ").await, Err(PostError::Empty), "пустой текст");
        assert_eq!(
            directory.asked.load(Ordering::SeqCst),
            0,
            "дешёвая локальная проверка идёт раньше похода по сети"
        );
    }
}
