mod avatars;
mod entry;
mod lifecycle;
mod memory;
mod outbox;
mod pg;
mod pg_files;
mod pg_life;
mod state;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use contracts::{Cause, OutboxRow, UserId, Username};
use journal::JournalError;
use tokio::sync::Notify;

use crate::common::{Avatar, ProfileChange, Relations, User};

pub use avatars::MemoryAvatars;
pub use entry::{UserEntry, UserLine};
pub use memory::MemoryUsers;
pub use pg::{MIGRATIONS, PgUsers, USERNAME_TAKEN};

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
    fn create_if_absent(
        &self,
        name: Username,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<User, RepoError>>;
    fn get(&self, id: UserId) -> BoxFuture<'_, Option<User>>;
    fn find(&self, name: Username) -> BoxFuture<'_, Option<User>>;
    fn list_after(&self, after: u64, limit: usize) -> BoxFuture<'_, Vec<User>>;
    fn follow(
        &self,
        who: UserId,
        whom: UserId,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<bool, JournalError>>;
    fn mute(
        &self,
        who: UserId,
        whom: UserId,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<bool, JournalError>>;
    fn relations(&self, id: UserId) -> BoxFuture<'_, Relations>;
    fn update(
        &self,
        id: UserId,
        change: ProfileChange,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Option<User>, JournalError>>;
}

pub trait LifecycleRepository: Send + Sync {
    fn hide(&self, id: UserId, cause: &Cause) -> BoxFuture<'_, Result<bool, JournalError>>;
    fn show(&self, id: UserId, cause: &Cause) -> BoxFuture<'_, Result<bool, JournalError>>;
    fn erase(&self, id: UserId, cause: &Cause) -> BoxFuture<'_, Result<bool, JournalError>>;
}

pub trait OutboxRepository: Send + Sync {
    fn unpublished(&self, limit: usize) -> BoxFuture<'_, Vec<OutboxRow>>;
    fn mark_published(&self, through: u64) -> BoxFuture<'_, Result<(), JournalError>>;
    fn waiting(&self) -> BoxFuture<'_, usize>;
    fn signal(&self) -> Arc<Notify>;
}

pub trait AvatarRepository: Send + Sync {
    fn link(&self, owner: UserId, avatar: Avatar) -> BoxFuture<'_, Option<Avatar>>;
    fn get(&self, owner: UserId) -> BoxFuture<'_, Option<Avatar>>;
    fn remove(&self, owner: UserId) -> BoxFuture<'_, Option<Avatar>>;
    fn referenced(&self, key: String) -> BoxFuture<'_, bool>;
    fn pending(&self, key: String, at: u64) -> BoxFuture<'_, ()>;
    fn settle(&self, key: String) -> BoxFuture<'_, ()>;
    fn stale(&self, before: u64) -> BoxFuture<'_, Vec<String>>;
}
