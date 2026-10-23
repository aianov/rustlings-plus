mod domain;
mod network;
mod ports;
mod storage;

pub use domain::{
    DomainError, MAX_NAME_LEN, MAX_POST_LEN, Post, PostId, PostKind, User, UserId, Visibility,
    normalize_name,
};
pub use network::Network;
pub use ports::{PostStore, UserStore};
pub use storage::{MemPostStore, MemUserStore};

pub type MemNetwork = Network<MemUserStore, MemPostStore>;
