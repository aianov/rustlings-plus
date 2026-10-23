mod pages;
mod stream;

pub use pages::{Pages, PagesExt};
pub use stream::{FeedEvent, new_posts, pages, watch, with_authors};
