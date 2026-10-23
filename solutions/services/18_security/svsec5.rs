
use std::collections::HashMap;

fn main() {}

#[derive(Debug, PartialEq, Eq)]
pub enum ApiError {
    Unauthorized,
    Forbidden,
    NotFound,
}

pub struct Post {
    pub author: u64,
    pub draft: bool,
    pub text: String,
}

pub struct Comment {
    pub post: u64,
    pub author: u64,
}

#[derive(Default)]
pub struct Store {
    pub posts: HashMap<u64, Post>,
    pub comments: HashMap<u64, Comment>,
}

pub fn delete_comment(store: &mut Store, user: Option<u64>, comment: u64) -> Result<(), ApiError> {
    let user = user.ok_or(ApiError::Unauthorized)?;
    let found = store.comments.get(&comment).ok_or(ApiError::NotFound)?;
    let post_author = store.posts.get(&found.post).map(|p| p.author);
    if found.author != user && post_author != Some(user) {
        return Err(ApiError::Forbidden);
    }
    store.comments.remove(&comment);
    Ok(())
}

pub fn get_draft(store: &Store, user: Option<u64>, post: u64) -> Result<String, ApiError> {
    let user = user.ok_or(ApiError::Unauthorized)?;
    match store.posts.get(&post) {
        Some(p) if p.draft && p.author == user => Ok(p.text.clone()),
        _ => Err(ApiError::NotFound),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let mut s = Store::default();
        s.posts.insert(1, Post { author: 10, draft: false, text: "публичный".into() });
        s.posts.insert(2, Post { author: 10, draft: true, text: "черновик Анны".into() });
        s.comments.insert(100, Comment { post: 1, author: 20 });
        s.comments.insert(101, Comment { post: 1, author: 30 });
        s
    }

    #[test]
    fn without_login_nothing_works() {
        let mut s = store();
        assert_eq!(delete_comment(&mut s, None, 100), Err(ApiError::Unauthorized));
        assert_eq!(get_draft(&s, None, 2), Err(ApiError::Unauthorized));
        assert!(s.comments.contains_key(&100));
    }

    #[test]
    fn a_stranger_cannot_delete_somebody_elses_comment() {
        let mut s = store();
        assert_eq!(delete_comment(&mut s, Some(99), 100), Err(ApiError::Forbidden), "посторонний удалил чужой комментарий");
        assert!(s.comments.contains_key(&100), "комментарий должен остаться на месте");
    }

    #[test]
    fn the_comment_author_and_the_post_author_may_delete() {
        let mut s = store();
        assert_eq!(delete_comment(&mut s, Some(20), 100), Ok(()), "автор комментария");
        assert_eq!(delete_comment(&mut s, Some(10), 101), Ok(()), "автор поста убирает комментарии под своим постом");
        assert_eq!(delete_comment(&mut s, Some(10), 100), Err(ApiError::NotFound), "уже удалён");
    }

    #[test]
    fn drafts_are_visible_to_their_author_only_and_look_nonexistent() {
        let s = store();
        assert_eq!(get_draft(&s, Some(10), 2).as_deref(), Ok("черновик Анны"));
        assert_eq!(get_draft(&s, Some(99), 2), Err(ApiError::NotFound), "чужой черновик не должен ни отдаваться, ни выдавать «запрещено»: это подтвердило бы, что он есть");
        assert_eq!(get_draft(&s, Some(99), 2), get_draft(&s, Some(99), 12345), "ответ про чужой черновик отличается от ответа про несуществующий пост");
        assert_eq!(get_draft(&s, Some(10), 1), Err(ApiError::NotFound), "опубликованный пост это не черновик");
    }
}
