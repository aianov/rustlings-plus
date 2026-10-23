
#[derive(Debug, PartialEq)]
pub enum RepoError {
    Duplicate,
    Timeout,
    Broken(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Post {
    pub id: u64,
    pub text: String,
}

pub trait PostRepo {
    fn find(&self, id: u64) -> Result<Option<Post>, RepoError>;
    fn insert(&self, post: &Post) -> Result<(), RepoError>;
}

#[derive(Debug, PartialEq)]
pub enum ServiceError {
    NotFound(u64),
    Duplicate(u64),
    TooLong { len: usize, max: usize },
    Unavailable,
    Internal(String),
}

#[derive(Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub code: &'static str,
}

const MAX_LEN: usize = 280;

fn from_repo(err: RepoError, id: u64) -> ServiceError {
    match err {
        RepoError::Duplicate => ServiceError::Duplicate(id),
        RepoError::Timeout => ServiceError::Unavailable,
        RepoError::Broken(text) => ServiceError::Internal(text),
    }
}

pub fn get_post(repo: &dyn PostRepo, id: u64) -> Result<Post, ServiceError> {
    repo.find(id).map_err(|e| from_repo(e, id))?.ok_or(ServiceError::NotFound(id))
}

pub fn create_post(repo: &dyn PostRepo, post: Post) -> Result<(), ServiceError> {
    let len = post.text.chars().count();
    if len > MAX_LEN {
        return Err(ServiceError::TooLong { len, max: MAX_LEN });
    }
    repo.insert(&post).map_err(|e| from_repo(e, post.id))
}

pub fn to_response(err: &ServiceError) -> Response {
    match err {
        ServiceError::NotFound(_) => Response { status: 404, code: "post.not_found" },
        ServiceError::Duplicate(_) => Response { status: 409, code: "post.duplicate" },
        ServiceError::TooLong { .. } => Response { status: 422, code: "post.too_long" },
        ServiceError::Unavailable => Response { status: 503, code: "service.unavailable" },
        ServiceError::Internal(_) => Response { status: 500, code: "internal" },
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Repo {
        find: RefCell<Option<Result<Option<Post>, RepoError>>>,
        insert: RefCell<Option<Result<(), RepoError>>>,
    }

    fn repo_find(r: Result<Option<Post>, RepoError>) -> Repo {
        Repo { find: RefCell::new(Some(r)), insert: RefCell::new(None) }
    }

    fn repo_insert(r: Result<(), RepoError>) -> Repo {
        Repo { find: RefCell::new(None), insert: RefCell::new(Some(r)) }
    }

    impl PostRepo for Repo {
        fn find(&self, _: u64) -> Result<Option<Post>, RepoError> {
            self.find.borrow_mut().take().expect("find не ожидался")
        }
        fn insert(&self, _: &Post) -> Result<(), RepoError> {
            self.insert.borrow_mut().take().expect("insert не ожидался")
        }
    }

    fn post(id: u64) -> Post {
        Post { id, text: "привет".to_string() }
    }

    fn status_of_get(r: Result<Option<Post>, RepoError>) -> u16 {
        match get_post(&repo_find(r), 5) {
            Ok(_) => 200,
            Err(e) => to_response(&e).status,
        }
    }

    fn status_of_create(post: Post, r: Result<(), RepoError>) -> u16 {
        match create_post(&repo_insert(r), post) {
            Ok(()) => 201,
            Err(e) => to_response(&e).status,
        }
    }

    #[test]
    fn found_and_missing() {
        assert_eq!(status_of_get(Ok(Some(post(5)))), 200, "пост есть");
        assert_eq!(status_of_get(Ok(None)), 404, "база ответила «такого нет»: вот это и есть 404");
    }

    #[test]
    fn database_trouble_is_not_a_missing_post() {
        assert_eq!(
            status_of_get(Err(RepoError::Timeout)),
            503,
            "таймаут базы это «попробуй позже», а не «поста нет»: клиент выбросит живой пост"
        );
        assert_eq!(status_of_get(Err(RepoError::Broken("x".into()))), 500, "сломанная база это наша ошибка");
    }

    #[test]
    fn create_reports_the_real_reason() {
        assert_eq!(status_of_create(post(1), Ok(())), 201, "успех");
        assert_eq!(status_of_create(post(1), Err(RepoError::Duplicate)), 409, "дубликат это конфликт, а не сбой сервера");
        assert_eq!(
            status_of_create(post(1), Err(RepoError::Timeout)),
            503,
            "таймаут при вставке не повод говорить «пост уже существует»"
        );
        assert_eq!(status_of_create(post(1), Err(RepoError::Broken("x".into()))), 500, "поломка базы");
    }

    #[test]
    fn too_long_text_is_the_clients_problem() {
        let long = Post { id: 1, text: "я".repeat(281) };
        let err = create_post(&repo_insert(Ok(())), long).unwrap_err();
        assert_eq!(err, ServiceError::TooLong { len: 281, max: 280 }, "длина считается в символах");
        assert_eq!(to_response(&err).status, 422, "слишком длинный текст это 422");
    }

    #[test]
    fn every_error_has_its_own_stable_code() {
        let all = [
            to_response(&ServiceError::NotFound(1)),
            to_response(&ServiceError::Duplicate(1)),
            to_response(&ServiceError::TooLong { len: 2, max: 1 }),
            to_response(&ServiceError::Unavailable),
            to_response(&ServiceError::Internal("x".into())),
        ];
        let mut codes: Vec<&str> = all.iter().map(|r| r.code).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), 5, "у разных ошибок разные коды, иначе клиент не отличит их: {all:?}");
    }
}
