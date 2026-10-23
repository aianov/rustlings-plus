mod error;

use std::sync::Arc;

use contracts::{UserId, Username, normalize_name};
use proto::AVATAR_MAX;

use crate::common::{Avatar, Relations, User};
use crate::repository::{AvatarRepository, RepoError, UserRepository};

pub use error::UserError;

pub const LIST_LIMIT: usize = 500;

pub struct UserService {
    users: Arc<dyn UserRepository>,
    avatars: Arc<dyn AvatarRepository>,
}

fn stored(error: RepoError, name: &Username) -> UserError {
    match error {
        RepoError::Conflict => UserError::NameTaken(name.as_ref().to_string()),
        RepoError::Storage(error) => UserError::Storage(error),
    }
}

impl UserService {
    pub fn new(users: Arc<dyn UserRepository>, avatars: Arc<dyn AvatarRepository>) -> UserService {
        UserService { users, avatars }
    }

    pub async fn register(&self, raw: &str) -> Result<User, UserError> {
        let name = Username::try_from(raw).map_err(UserError::BadName)?;
        let created = self.users.create_if_absent(name.clone()).await;
        created.map_err(|error| stored(error, &name))
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

    pub async fn follow(&self, who: UserId, whom: UserId) -> Result<bool, UserError> {
        if who == whom {
            return Err(UserError::SelfFollow);
        }
        self.pair(who, whom).await?;
        self.users.follow(who, whom).await.map_err(UserError::from)
    }

    pub async fn mute(&self, who: UserId, whom: UserId) -> Result<bool, UserError> {
        if who == whom {
            return Err(UserError::SelfMute);
        }
        self.pair(who, whom).await?;
        self.users.mute(who, whom).await.map_err(UserError::from)
    }

    pub async fn relations(&self, id: UserId) -> Result<Relations, UserError> {
        self.profile(id).await?;
        Ok(self.users.relations(id).await)
    }

    pub async fn put_avatar(
        &self,
        owner: UserId,
        content_type: String,
        bytes: Vec<u8>,
    ) -> Result<bool, UserError> {
        self.profile(owner).await?;
        if !content_type.starts_with("image/") {
            return Err(UserError::NotImage(content_type));
        }
        if bytes.len() > AVATAR_MAX {
            return Err(UserError::TooLarge(AVATAR_MAX));
        }
        let avatar = Avatar {
            content_type,
            bytes: Arc::new(bytes),
        };
        Ok(self.avatars.put(owner, avatar).await)
    }

    pub async fn avatar(&self, owner: UserId) -> Result<Avatar, UserError> {
        self.avatars
            .get(owner)
            .await
            .ok_or(UserError::NoAvatar(owner))
    }

    pub async fn delete_avatar(&self, owner: UserId) -> bool {
        self.avatars.remove(owner).await
    }
}
