use crate::comments::Comment;
use crate::domain::{CommentId, DomainError, PostId, UserId};
use crate::ports::{PostStore, UserStore};

use super::Network;

impl<U: UserStore, P: PostStore> Network<U, P> {
    pub fn comment(
        &mut self,
        author: UserId,
        post: PostId,
        text: &str,
    ) -> Result<CommentId, DomainError> {
        self.ensure_user(author)?;
        self.ensure_visible(author, post)?;
        self.comments.add(post, author, None, text)
    }

    pub fn reply(
        &mut self,
        author: UserId,
        parent: CommentId,
        text: &str,
    ) -> Result<CommentId, DomainError> {
        self.ensure_user(author)?;
        let post = self
            .comments
            .get(parent)
            .map(Comment::post)
            .ok_or(DomainError::UnknownComment(parent))?;
        self.ensure_visible(author, post)?;
        self.comments.add(post, author, Some(parent), text)
    }

    pub fn find_comment(&self, id: CommentId) -> Option<&Comment> {
        self.comments.get(id)
    }

    pub fn thread(
        &self,
        viewer: UserId,
        post: PostId,
    ) -> Result<Vec<(usize, &Comment)>, DomainError> {
        self.ensure_user(viewer)?;
        self.ensure_visible(viewer, post)?;
        Ok(self.comments.thread(post))
    }

    pub fn replies(
        &self,
        viewer: UserId,
        post: PostId,
        parent: Option<CommentId>,
    ) -> Result<Vec<&Comment>, DomainError> {
        self.ensure_user(viewer)?;
        self.ensure_visible(viewer, post)?;
        Ok(self.comments.children(post, parent))
    }
}
