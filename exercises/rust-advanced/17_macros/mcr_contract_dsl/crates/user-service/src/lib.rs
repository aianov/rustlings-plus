pub struct RegisterRequest {
    pub login: String,
}

pub struct RegisterResponse {
    pub id: u64,
}

pub struct FollowRequest {
    pub user: u64,
}

pub struct Empty;

contracts::service! {
    /// Методы сервиса пользователей.
    pub enum UserMethod {
        Register = "users.register" (RegisterRequest) -> RegisterResponse,
        Follow = "users.follow" (FollowRequest) -> Empty,
        Unfollow = "users.unfollow" (FollowRequest) -> Empty,
    }
}

pub fn dispatch(name: &str) -> Result<UserMethod, String> {
    UserMethod::from_name(name).ok_or_else(|| format!("неизвестный метод {name}"))
}
