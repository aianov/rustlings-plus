mod error;
mod post;
mod user;

pub use contracts::{
    Clock, CommentId, Cursor, MAX_NAME_LEN, PostId, SystemClock, Timestamp, UserId, Visibility,
    normalize_name,
};
pub use error::DomainError;
pub use post::{MAX_POST_LEN, PREVIEW_LEN, Post, PostKind, preview, validate_text};
pub use user::{User, validate_name};
