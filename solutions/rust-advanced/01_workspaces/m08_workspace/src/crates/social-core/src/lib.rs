mod commands;
mod domain;
mod network;
mod notify;
mod ports;
mod shared;
mod storage;

pub use commands::{CommandError, execute, execute_line};
pub use domain::{
    DomainError, MAX_NAME_LEN, MAX_POST_LEN, Post, PostId, PostKind, User, UserId, Visibility,
    normalize_name,
};
pub use network::Network;
pub use notify::Notification;
pub use ports::{PostStore, UserStore};
pub use shared::SharedNetwork;
pub use storage::{MemPostStore, MemUserStore};

pub type MemNetwork = Network<MemUserStore, MemPostStore>;
