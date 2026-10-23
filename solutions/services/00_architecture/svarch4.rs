#[derive(Clone, Debug, PartialEq)]
pub struct Post {
    pub id: u64,
    pub author: u64,
    pub text: String,
}

pub trait PostRepository {
    fn insert(&self, author: u64, text: String) -> Post;
}

pub trait FollowRepository {
    fn add(&self, follower: u64, target: u64) -> bool;
}

#[derive(Debug, PartialEq)]
pub enum PostError {
    Empty,
    TooLong { max: usize },
}

#[derive(Debug, PartialEq)]
pub enum FollowError {
    SelfFollow,
    AlreadyFollowing,
}

pub const MAX_POST_CHARS: usize = 280;

pub struct PostService<R: PostRepository> {
    repo: R,
}

impl<R: PostRepository> PostService<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }

    pub fn create(&self, author: u64, text: &str) -> Result<Post, PostError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(PostError::Empty);
        }
        if text.chars().count() > MAX_POST_CHARS {
            return Err(PostError::TooLong { max: MAX_POST_CHARS });
        }
        Ok(self.repo.insert(author, text.to_string()))
    }
}

pub struct FollowService<R: FollowRepository> {
    repo: R,
}

impl<R: FollowRepository> FollowService<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }

    pub fn follow(&self, follower: u64, target: u64) -> Result<(), FollowError> {
        if follower == target {
            return Err(FollowError::SelfFollow);
        }
        if self.repo.add(follower, target) {
            Ok(())
        } else {
            Err(FollowError::AlreadyFollowing)
        }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct CountingPosts {
        calls: AtomicUsize,
        saved: Mutex<Vec<Post>>,
    }

    impl PostRepository for &CountingPosts {
        fn insert(&self, author: u64, text: String) -> Post {
            let n = self.calls.fetch_add(1, Ordering::SeqCst) as u64 + 1;
            let post = Post { id: n, author, text };
            self.saved.lock().unwrap().push(post.clone());
            post
        }
    }

    #[derive(Default)]
    struct CountingFollows {
        calls: AtomicUsize,
        pairs: Mutex<HashSet<(u64, u64)>>,
    }

    impl FollowRepository for &CountingFollows {
        fn add(&self, follower: u64, target: u64) -> bool {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.pairs.lock().unwrap().insert((follower, target))
        }
    }

    #[test]
    fn post_text_is_trimmed_before_saving() {
        let repo = CountingPosts::default();
        let service = PostService::new(&repo);
        let post = service.create(7, "  привет, мир \n").unwrap();
        assert_eq!(post.text, "привет, мир", "в репозиторий должен уйти обрезанный текст");
        assert_eq!(post.author, 7, "автор передаётся как есть");
        assert_eq!(repo.saved.lock().unwrap().len(), 1, "пост должен быть сохранён ровно один раз");
    }

    #[test]
    fn empty_post_never_reaches_the_repository() {
        let repo = CountingPosts::default();
        let service = PostService::new(&repo);
        assert_eq!(service.create(1, " \t\n"), Err(PostError::Empty), "пост из пробелов пустой");
        assert_eq!(repo.calls.load(Ordering::SeqCst), 0, "правило проверяется ДО репозитория");
    }

    #[test]
    fn limit_is_counted_in_chars_not_bytes() {
        let repo = CountingPosts::default();
        let service = PostService::new(&repo);
        assert!(
            service.create(1, &"я".repeat(280)).is_ok(),
            "280 символов влезают, хотя это 560 байт: считай chars()"
        );
        assert_eq!(
            service.create(1, &"я".repeat(281)),
            Err(PostError::TooLong { max: 280 }),
            "281 символ уже много, ошибка называет лимит"
        );
        assert_eq!(repo.calls.load(Ordering::SeqCst), 1, "длинный пост не сохраняется");
    }

    #[test]
    fn cannot_follow_yourself() {
        let repo = CountingFollows::default();
        let service = FollowService::new(&repo);
        assert_eq!(service.follow(5, 5), Err(FollowError::SelfFollow), "подписка на себя запрещена");
        assert_eq!(repo.calls.load(Ordering::SeqCst), 0, "service отказывает, не трогая репозиторий");
    }

    #[test]
    fn second_follow_is_a_conflict() {
        let repo = CountingFollows::default();
        let service = FollowService::new(&repo);
        assert_eq!(service.follow(1, 2), Ok(()), "первая подписка проходит");
        assert_eq!(
            service.follow(1, 2),
            Err(FollowError::AlreadyFollowing),
            "репозиторий вернул false: подписка уже была"
        );
        assert_eq!(service.follow(2, 1), Ok(()), "обратная подписка это другая пара");
    }
}
