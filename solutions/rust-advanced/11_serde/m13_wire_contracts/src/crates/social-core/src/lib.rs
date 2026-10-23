mod api;
mod chat;
mod commands;
mod domain;
mod error;
mod events;
mod feed;
mod network;
mod notify;
mod ports;
mod shared;
mod storage;

pub use api::{serve, serve_line};
pub use chat::{MAX_MESSAGE_LEN, Message};
pub use commands::{
    CommandBus, CommandError, Context, IntoResponse, Response, execute, execute_line,
    network_commands,
};
pub use domain::{
    DomainError, MAX_NAME_LEN, MAX_POST_LEN, Post, PostId, PostKind, User, UserId, Visibility,
    normalize_name,
};
pub use error::{Error, Result, report};
pub use events::{Event, EventBus, SubId, Topic};
pub use feed::{Pages, PagesExt};
pub use network::Network;
pub use notify::{LogNotifier, Notification, Notifier, Notifiers, PushQueue};
pub use ports::{PostStore, UserStore};
pub use shared::SharedNetwork;
pub use storage::{MemPostStore, MemUserStore};

pub type MemNetwork = Network<MemUserStore, MemPostStore>;
