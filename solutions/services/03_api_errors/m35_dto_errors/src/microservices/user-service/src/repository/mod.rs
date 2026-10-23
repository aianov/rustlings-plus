mod avatars;
mod memory;

use std::future::Future;
use std::pin::Pin;

use contracts::{UserId, Username};
use journal::JournalError;

use crate::common::{Avatar, ProfileChange, Relations, User};

pub use avatars::MemoryAvatars;
pub use memory::MemoryUsers;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug)]
pub enum RepoError {
    Conflict,
    Storage(JournalError),
}

impl From<JournalError> for RepoError {
    fn from(error: JournalError) -> RepoError {
        RepoError::Storage(error)
    }
}

pub trait UserRepository: Send + Sync {
    fn create_if_absent(&self, name: Username) -> BoxFuture<'_, Result<User, RepoError>>;
    fn get(&self, id: UserId) -> BoxFuture<'_, Option<User>>;
    fn find(&self, name: Username) -> BoxFuture<'_, Option<User>>;
    fn list_after(&self, after: u64, limit: usize) -> BoxFuture<'_, Vec<User>>;
    fn follow(&self, who: UserId, whom: UserId) -> BoxFuture<'_, Result<bool, JournalError>>;
    fn mute(&self, who: UserId, whom: UserId) -> BoxFuture<'_, Result<bool, JournalError>>;
    fn relations(&self, id: UserId) -> BoxFuture<'_, Relations>;
    fn update(
        &self,
        id: UserId,
        change: ProfileChange,
    ) -> BoxFuture<'_, Result<Option<User>, JournalError>>;
}

pub trait AvatarRepository: Send + Sync {
    fn put(&self, owner: UserId, avatar: Avatar) -> BoxFuture<'_, bool>;
    fn get(&self, owner: UserId) -> BoxFuture<'_, Option<Avatar>>;
    fn remove(&self, owner: UserId) -> BoxFuture<'_, bool>;
}
