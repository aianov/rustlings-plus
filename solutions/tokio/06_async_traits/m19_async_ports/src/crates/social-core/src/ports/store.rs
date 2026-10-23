use crate::domain::{Post, PostId, User, UserId};

pub trait UserStore {
    fn next_id(&mut self) -> UserId;
    fn insert(&mut self, user: User);
    fn get(&self, id: UserId) -> Option<&User>;
    fn id_by_name(&self, name: &str) -> Option<UserId>;
    fn count(&self) -> usize;

    fn contains(&self, id: UserId) -> bool {
        self.get(id).is_some()
    }

    fn is_taken(&self, name: &str) -> bool {
        self.id_by_name(name).is_some()
    }

    fn find_by_name(&self, name: &str) -> Option<&User> {
        let id = self.id_by_name(name)?;
        self.get(id)
    }
}

pub trait PostStore {
    fn next_id(&mut self) -> PostId;
    fn insert(&mut self, post: Post);
    fn get(&self, id: PostId) -> Option<&Post>;
    fn get_mut(&mut self, id: PostId) -> Option<&mut Post>;
    fn all(&self) -> &[Post];

    fn count(&self) -> usize {
        self.all().len()
    }

    fn contains(&self, id: PostId) -> bool {
        self.get(id).is_some()
    }

    fn last_by(&self, author: UserId) -> Option<&Post> {
        self.all().iter().rev().find(|post| post.author() == author)
    }
}
