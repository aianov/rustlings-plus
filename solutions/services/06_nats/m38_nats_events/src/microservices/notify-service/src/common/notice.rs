use contracts::{EventKind, NoticeKind, NoticeView, PostId, UserId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incoming {
    pub kind: EventKind,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub id: u64,
    pub to: UserId,
    pub kind: NoticeKind,
    pub actor: UserId,
    pub post: Option<PostId>,
    pub read: bool,
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
        }
    }

    pub fn same(&self, draft: &Draft) -> bool {
        !self.read
            && self.to == draft.to
            && self.kind == draft.kind
            && self.actor == draft.actor
            && self.post == draft.post
    }
}
