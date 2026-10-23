mod avatars;
mod error;
mod social;

use std::sync::Arc;

use contracts::{Cause, Origin, UserId, Username, normalize_name};
use files::FileStore;

use crate::common::{ProfileChange, Relations, User, now_micros};
use crate::repository::{
    AvatarRepository, LifecycleRepository, RepoError, SocialQueries, UserRepository,
};

pub use avatars::{AVATAR_CHUNK, ORPHAN_GRACE};
pub use error::UserError;
pub use social::Related;

pub const LIST_LIMIT: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fate {
    Hide,
    Show,
    Erase,
}

pub struct UserService {
    users: Arc<dyn UserRepository>,
    avatars: Arc<dyn AvatarRepository>,
    lifecycle: Option<Arc<dyn LifecycleRepository>>,
    files: Option<Arc<dyn FileStore>>,
    queries: Option<Arc<dyn SocialQueries>>,
}

fn stored(error: RepoError, name: &Username) -> UserError {
    match error {
        RepoError::Conflict => UserError::NameTaken(name.as_ref().to_string()),
        RepoError::Storage(error) => UserError::Storage(error),
    }
}

impl UserService {
    pub fn new(users: Arc<dyn UserRepository>, avatars: Arc<dyn AvatarRepository>) -> UserService {
        UserService {
            users,
            avatars,
            lifecycle: None,
            files: None,
            queries: None,
        }
    }

    pub fn with_files(self, files: Arc<dyn FileStore>) -> UserService {
        UserService {
            files: Some(files),
            ..self
        }
    }

    pub fn with_queries(self, queries: Arc<dyn SocialQueries>) -> UserService {
        UserService {
            queries: Some(queries),
            ..self
        }
    }

    pub fn with_lifecycle(self, lifecycle: Arc<dyn LifecycleRepository>) -> UserService {
        UserService {
            lifecycle: Some(lifecycle),
            ..self
        }
    }

    pub async fn settle(
        &self,
        who: UserId,
        fate: Fate,
        origin: &Origin,
    ) -> Result<bool, UserError> {
        let lifecycle = self.lifecycle.as_ref().ok_or(UserError::NoLifecycle)?;
        let cause = Cause::new(origin, now_micros());
        let changed = match fate {
            Fate::Hide => lifecycle.hide(who, &cause).await?,
            Fate::Show => lifecycle.show(who, &cause).await?,
            Fate::Erase => lifecycle.erase(who, &cause).await?,
        };
        Ok(changed)
    }

    pub async fn register(&self, name: Username, origin: &Origin) -> Result<User, UserError> {
        let cause = Cause::new(origin, now_micros());
        let created = self.users.create_if_absent(name.clone(), &cause).await;
        created.map_err(|error| stored(error, &name))
    }

    pub async fn update(
        &self,
        who: UserId,
        change: ProfileChange,
        origin: &Origin,
    ) -> Result<User, UserError> {
        let cause = Cause::new(origin, now_micros());
        let updated = self.users.update(who, change, &cause).await?;
        updated.ok_or(UserError::UnknownUser(who))
    }

    pub async fn profile(&self, id: UserId) -> Result<User, UserError> {
        self.users.get(id).await.ok_or(UserError::UnknownUser(id))
    }

    pub async fn find(&self, raw: &str) -> Result<User, UserError> {
        let unknown = || UserError::UnknownName(normalize_name(raw).into_owned());
        let name = Username::try_from(raw).map_err(|_| unknown())?;
        self.users.find(name).await.ok_or_else(unknown)
    }

    pub async fn list(&self, after: u64, limit: usize) -> Vec<User> {
        let limit = if limit == 0 {
            LIST_LIMIT
        } else {
            limit.min(LIST_LIMIT)
        };
        self.users.list_after(after, limit).await
    }

    async fn pair(&self, who: UserId, whom: UserId) -> Result<(), UserError> {
        self.profile(who).await?;
        self.profile(whom).await?;
        Ok(())
    }

    pub async fn follow(
        &self,
        who: UserId,
        whom: UserId,
        origin: &Origin,
    ) -> Result<bool, UserError> {
        if who == whom {
            return Err(UserError::SelfFollow);
        }
        self.pair(who, whom).await?;
        let cause = Cause::new(origin, now_micros());
        Ok(self.users.follow(who, whom, &cause).await?)
    }

    pub async fn mute(
        &self,
        who: UserId,
        whom: UserId,
        origin: &Origin,
    ) -> Result<bool, UserError> {
        if who == whom {
            return Err(UserError::SelfMute);
        }
        self.pair(who, whom).await?;
        let cause = Cause::new(origin, now_micros());
        Ok(self.users.mute(who, whom, &cause).await?)
    }

    pub async fn relations(&self, id: UserId) -> Result<Relations, UserError> {
        self.profile(id).await?;
        Ok(self.users.relations(id).await)
    }
}
