mod domain;
mod network;

pub use domain::{
    DomainError, MAX_NAME_LEN, MAX_POST_LEN, Post, PostId, PostKind, User, UserId, Visibility,
    normalize_name,
};
pub use network::Network;
