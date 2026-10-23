use std::collections::HashMap;

use crate::domain::{CommentId, DomainError, PostId, UserId};

pub const MAX_COMMENT_LEN: usize = 280;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    id: CommentId,
    post: PostId,
    author: UserId,
    parent: Option<CommentId>,
    text: String,
}

impl Comment {
    pub fn id(&self) -> CommentId {
        self.id
    }

    pub fn post(&self) -> PostId {
        self.post
    }

    pub fn author(&self) -> UserId {
        self.author
    }

    pub fn parent(&self) -> Option<CommentId> {
        self.parent
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Debug, Default)]
pub struct Comments {
    all: HashMap<CommentId, Comment>,
    by_post: HashMap<PostId, Vec<CommentId>>,
    last_id: u64,
}

impl Comments {
    pub fn add(
        &mut self,
        post: PostId,
        author: UserId,
        parent: Option<CommentId>,
        text: &str,
    ) -> Result<CommentId, DomainError> {
        let text = validate(text)?;
        self.last_id += 1;
        let id = CommentId(self.last_id);
        self.by_post.entry(post).or_default().push(id);
        let comment = Comment {
            id,
            post,
            author,
            parent,
            text,
        };
        self.all.insert(id, comment);
        Ok(id)
    }

    pub fn get(&self, id: CommentId) -> Option<&Comment> {
        self.all.get(&id)
    }

    pub fn children(&self, post: PostId, parent: Option<CommentId>) -> Vec<&Comment> {
        let ids = self.by_post.get(&post).into_iter().flatten();
        ids.filter_map(|id| self.all.get(id))
            .filter(|comment| comment.parent == parent)
            .collect()
    }

    pub fn forget_post(&mut self, post: PostId) {
        for id in self.by_post.remove(&post).unwrap_or_default() {
            self.all.remove(&id);
        }
    }

    pub fn thread(&self, post: PostId) -> Vec<(usize, &Comment)> {
        let mut replies: HashMap<Option<CommentId>, Vec<&Comment>> = HashMap::new();
        let ids = self.by_post.get(&post).into_iter().flatten();
        for comment in ids.filter_map(|id| self.all.get(id)) {
            replies.entry(comment.parent).or_default().push(comment);
        }
        let mut stack: Vec<(usize, &Comment)> = Vec::new();
        push_level(&mut stack, replies.get(&None), 0);
        let mut thread = Vec::new();
        while let Some((depth, comment)) = stack.pop() {
            thread.push((depth, comment));
            push_level(&mut stack, replies.get(&Some(comment.id)), depth + 1);
        }
        thread
    }
}

fn push_level<'a>(
    stack: &mut Vec<(usize, &'a Comment)>,
    level: Option<&Vec<&'a Comment>>,
    depth: usize,
) {
    let level = level.into_iter().flatten().rev();
    stack.extend(level.map(|comment| (depth, *comment)));
}

fn validate(text: &str) -> Result<String, DomainError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(DomainError::EmptyComment);
    }
    if text.chars().count() > MAX_COMMENT_LEN {
        return Err(DomainError::CommentTooLong);
    }
    Ok(text.to_string())
}
