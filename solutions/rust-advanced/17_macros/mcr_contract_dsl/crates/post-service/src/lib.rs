pub struct CreateRequest {
    pub text: String,
}

pub struct Post {
    pub id: u64,
}

pub struct Page<T> {
    pub items: Vec<T>,
}

pub struct ListRequest {
    pub cursor: Option<String>,
}

contracts::service! {
    /// Методы сервиса постов.
    pub enum PostMethod {
        Create = "posts.create" (CreateRequest) -> Post,
        List = "posts.list" (ListRequest) -> Page<Post>,
    }
}

pub fn dispatch(name: &str) -> Result<PostMethod, String> {
    PostMethod::from_name(name).ok_or_else(|| format!("неизвестный метод {name}"))
}
