use contracts::{NoticeKind, NoticeView, PostId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Followed,
    Liked,
    Commented,
    Posted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incoming {
    pub kind: Trigger,
    pub owner: UserId,
    pub actor: UserId,
    pub post: Option<PostId>,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub to: UserId,
    pub kind: NoticeKind,
    pub actor: UserId,
    pub post: Option<PostId>,
    pub count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub id: u64,
    pub to: UserId,
    pub kind: NoticeKind,
    pub actor: UserId,
    pub post: Option<PostId>,
    pub read: bool,
    pub count: Option<u32>,
}

impl Notice {
    pub fn view(&self) -> NoticeView {
        NoticeView {
            id: self.id,
            to: self.to,
            kind: self.kind,
            actor: self.actor,
            post: self.post,
            read: self.read,
            count: self.count,
        }
    }

    pub fn same(&self, draft: &Draft) -> bool {
        let unread_twin = !self.read && self.to == draft.to && self.kind == draft.kind;
        match draft.kind {
            NoticeKind::Digest => unread_twin,
            NoticeKind::Followed
            | NoticeKind::Liked
            | NoticeKind::Commented
            | NoticeKind::NewPost => {
                unread_twin && self.actor == draft.actor && self.post == draft.post
            }
        }
    }
}
