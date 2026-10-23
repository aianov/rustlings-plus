mod api;
mod chat;
mod commands;
mod comments;
mod domain;
mod error;
mod events;
mod feed;
mod network;
mod notify;
mod ports;
mod shared;
mod storage;
mod trends;

pub use api::{serve, serve_line};
pub use chat::{MAX_MESSAGE_LEN, Message};
pub use commands::{
    CommandBus, CommandError, Context, IntoResponse, Response, execute, execute_line,
    network_commands,
};
pub use comments::{Comment, MAX_COMMENT_LEN};
pub use domain::{
    CommentId, Cursor, DomainError, MAX_NAME_LEN, MAX_POST_LEN, PREVIEW_LEN, Post, PostId,
    PostKind, Timestamp, User, UserId, Visibility, normalize_name, preview,
};
pub use error::{Error, Result, report};
pub use events::{Event, EventBus, SubId, Topic};
pub use feed::{Pages, PagesExt};
pub use network::{AuthorPage, Network};
pub use notify::{LogNotifier, Notification, Notifier, Notifiers, PushQueue};
pub use ports::{PostStore, UserStore};
pub use shared::SharedNetwork;
pub use storage::{MemPostStore, MemUserStore};
pub use trends::{TRENDS_WINDOW, Trend};

pub type MemNetwork = Network<MemUserStore, MemPostStore>;
