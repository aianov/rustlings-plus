#![allow(dead_code)]

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UserId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PostId(u64);

#[derive(Debug)]
struct User {
    id: UserId,
    name: String,
    bio: String,
}

impl User {
    fn new(id: UserId, name: String) -> User {
        User {
            id,
            name,
            bio: String::new(),
        }
    }

    fn id(&self) -> UserId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn bio(&self) -> &str {
        &self.bio
    }

    fn rename(&mut self, name: String) {
        self.name = name;
    }

    fn set_bio(&mut self, bio: String) {
        self.bio = bio;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Visibility {
    Public,
    Followers,
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PostKind {
    Text,
    Repost(PostId),
}

#[derive(Debug)]
struct Post {
    id: PostId,
    author: UserId,
    text: String,
    kind: PostKind,
    visibility: Visibility,
    likes: u32,
}

impl Post {
    fn new(id: PostId, author: UserId, text: String, visibility: Visibility) -> Post {
        Post {
            id,
            author,
            text,
            kind: PostKind::Text,
            visibility,
            likes: 0,
        }
    }

    fn repost(id: PostId, author: UserId, original: PostId) -> Post {
        Post {
            id,
            author,
            text: String::new(),
            kind: PostKind::Repost(original),
            visibility: Visibility::Public,
            likes: 0,
        }
    }

    fn id(&self) -> PostId {
        self.id
    }

    fn author(&self) -> UserId {
        self.author
    }

    fn text(&self) -> &str {
        &self.text
    }

    fn kind(&self) -> PostKind {
        self.kind
    }

    fn visibility(&self) -> Visibility {
        self.visibility
    }

    fn likes(&self) -> u32 {
        self.likes
    }

    fn like(&mut self) {
        self.likes += 1;
    }

    fn is_repost(&self) -> bool {
        matches!(self.kind, PostKind::Repost(_))
    }

    fn can_view(&self, viewer: UserId, follows_author: bool) -> bool {
        match self.visibility {
            Visibility::Public => true,
            Visibility::Followers => viewer == self.author || follows_author,
            Visibility::Private => viewer == self.author,
        }
    }
}
