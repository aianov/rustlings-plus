use super::UserId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PostId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Followers,
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostKind {
    Text,
    Repost(PostId),
}

#[derive(Debug)]
pub struct Post {
    id: PostId,
    author: UserId,
    text: String,
    kind: PostKind,
    visibility: Visibility,
    liked_by: Vec<UserId>,
}

impl Post {
    pub fn new(id: PostId, author: UserId, text: String, visibility: Visibility) -> Post {
        Post {
            id,
            author,
            text,
            kind: PostKind::Text,
            visibility,
            liked_by: Vec::new(),
        }
    }

    pub fn repost(id: PostId, author: UserId, original: PostId) -> Post {
        Post {
            id,
            author,
            text: String::new(),
            kind: PostKind::Repost(original),
            visibility: Visibility::Public,
            liked_by: Vec::new(),
        }
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

    pub fn likes(&self) -> usize {
        self.liked_by.len()
    }

    pub fn is_liked_by(&self, user: UserId) -> bool {
        self.liked_by.contains(&user)
    }

    pub fn like(&mut self, by: UserId) -> bool {
        if self.is_liked_by(by) {
            return false;
        }
        self.liked_by.push(by);
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
