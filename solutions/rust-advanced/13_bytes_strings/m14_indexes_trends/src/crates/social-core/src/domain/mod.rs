mod clock;
mod error;
mod post;
mod user;

pub use clock::now;
pub use contracts::{CommentId, Cursor, MAX_NAME_LEN, PostId, Timestamp, UserId, normalize_name};
pub use error::DomainError;
pub use post::{MAX_POST_LEN, PREVIEW_LEN, Post, PostKind, Visibility, preview, validate_text};
pub use user::{User, validate_name};
