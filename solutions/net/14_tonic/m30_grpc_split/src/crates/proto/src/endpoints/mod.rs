use std::env;

pub const AVATAR_MAX: usize = 8 * 1024 * 1024;

pub const MESSAGE_MAX: usize = AVATAR_MAX + 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Endpoint {
    pub env: &'static str,
    pub default: &'static str,
}

pub const AUTH: Endpoint = Endpoint {
    env: "AUTH_GRPC_ADDR",
    default: "127.0.0.1:50051",
};

pub const USERS: Endpoint = Endpoint {
    env: "USERS_GRPC_ADDR",
    default: "127.0.0.1:50052",
};

pub const POSTS: Endpoint = Endpoint {
    env: "POSTS_GRPC_ADDR",
    default: "127.0.0.1:50053",
};

impl Endpoint {
    pub fn addr(&self) -> String {
        env::var(self.env).unwrap_or_else(|_| self.default.to_string())
    }
}
