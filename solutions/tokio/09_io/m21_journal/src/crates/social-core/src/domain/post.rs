use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

use super::{Cursor, DomainError, PostId, Timestamp, UserId, Visibility};

pub const MAX_POST_LEN: usize = 280;

pub const PREVIEW_LEN: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostKind {
    Text,
    Repost(PostId),
}

#[derive(Debug)]
pub struct Post {
    id: PostId,
    author: UserId,
    text: Arc<str>,
    kind: PostKind,
    visibility: Visibility,
    time: Timestamp,
    liked_by: HashSet<UserId>,
    deleted: bool,
}

impl Post {
    pub fn new(id: PostId, author: UserId, text: String, visibility: Visibility) -> Post {
        Post {
            id,
            author,
            text: text.into(),
            kind: PostKind::Text,
            visibility,
            time: 0,
            liked_by: HashSet::new(),
            deleted: false,
        }
    }

    pub fn repost(id: PostId, author: UserId, original: PostId) -> Post {
        Post {
            id,
            author,
            text: Arc::from(""),
            kind: PostKind::Repost(original),
            visibility: Visibility::Public,
            time: 0,
            liked_by: HashSet::new(),
            deleted: false,
        }
    }

    pub fn at(mut self, time: Timestamp) -> Post {
        self.time = time;
        self
    }

    pub fn id(&self) -> PostId {
        self.id
    }

    pub fn author(&self) -> UserId {
        self.author
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn kind(&self) -> PostKind {
        self.kind
    }

    pub fn visibility(&self) -> Visibility {
        self.visibility
    }

    pub fn time(&self) -> Timestamp {
        self.time
    }

    pub fn cursor(&self) -> Cursor {
        Cursor {
            time: self.time,
            post: self.id,
        }
    }

    pub fn shared_text(&self) -> Arc<str> {
        Arc::clone(&self.text)
    }

    pub fn preview(&self) -> Cow<'_, str> {
        preview(&self.text, PREVIEW_LEN)
    }

    pub fn likes(&self) -> usize {
        self.liked_by.len()
    }

    pub fn is_liked_by(&self, user: UserId) -> bool {
        self.liked_by.contains(&user)
    }

    pub fn like(&mut self, by: UserId) -> bool {
        self.liked_by.insert(by)
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted
    }

    pub(crate) fn delete(&mut self) -> bool {
        if self.deleted {
            return false;
        }
        self.deleted = true;
        true
    }

    pub fn is_repost(&self) -> bool {
        matches!(self.kind, PostKind::Repost(_))
    }

    pub fn can_view(&self, viewer: UserId, follows_author: bool) -> bool {
        match self.visibility {
            Visibility::Public => true,
            Visibility::Followers => viewer == self.author || follows_author,
            Visibility::Private => viewer == self.author,
        }
    }
}

impl fmt::Display for Post {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            PostKind::Text => write!(f, "[{}] {}", self.id.0, self.preview())?,
            PostKind::Repost(original) => write!(f, "[{}] репост [{}]", self.id.0, original.0)?,
        }
        write!(f, " (лайков: {})", self.likes())
    }
}

pub fn validate_text(text: &str) -> Result<String, DomainError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(DomainError::EmptyPost);
    }
    if text.chars().count() > MAX_POST_LEN {
        return Err(DomainError::PostTooLong);
    }
    Ok(text.to_string())
}

pub fn preview(text: &str, max_bytes: usize) -> Cow<'_, str> {
    if text.len() <= max_bytes {
        return Cow::Borrowed(text);
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Cow::Owned(format!("{}…", text[..end].trim_end()))
}
