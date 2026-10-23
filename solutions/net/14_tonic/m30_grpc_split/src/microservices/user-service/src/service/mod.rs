use std::sync::Arc;

use proto::social::v1::users_server::Users;
use proto::social::v1::{
    Avatar as AvatarReply, AvatarRequest, Changed, CreateProfileRequest, FindProfileRequest,
    GetProfileRequest, ListProfilesRequest, PairRequest, Profile, ProfileList, PutAvatarReply,
    PutAvatarRequest, RelationsReply, RelationsRequest,
};
use proto::{AVATAR_MAX, domain, request_id};
use social_core::{App, DomainError, User, UserId, report};
use tonic::{Request, Response, Status};

use crate::avatars::{Avatar, Avatars};

pub const LIST_LIMIT: u32 = 500;

pub struct UsersService {
    app: App,
    avatars: Avatars,
}

impl UsersService {
    pub fn new(app: App) -> UsersService {
        UsersService {
            app,
            avatars: Avatars::default(),
        }
    }

    async fn existing(&self, id: u64, rid: &str) -> Result<User, Status> {
        let id = UserId(id);
        match self.app.user(id).await {
            Some(user) => Ok(user),
            None => Err(failed(DomainError::UnknownUser(id), rid)),
        }
    }
}

fn failed(error: impl Into<social_core::Error>, rid: &str) -> Status {
    let error = error.into();
    let code = error.code();
    if code == "internal" {
        eprintln!("user-service [{rid}]: {}", report(&error));
    }
    domain(code, &report(&error))
}

fn profile(user: &User) -> Profile {
    Profile {
        id: user.id().0,
        username: user.name().to_string(),
    }
}

fn ids(users: &[UserId]) -> Vec<u64> {
    users.iter().map(|user| user.0).collect()
}

fn changed(changed: bool) -> Response<Changed> {
    Response::new(Changed { changed })
}

#[tonic::async_trait]
impl Users for UsersService {
    async fn create_profile(
        &self,
        request: Request<CreateProfileRequest>,
    ) -> Result<Response<Profile>, Status> {
        let rid = request_id(request.metadata());
        let name = request.into_inner().username;
        let id = self
            .app
            .register(&name)
            .await
            .map_err(|error| failed(error, &rid))?;
        let user = self.existing(id.0, &rid).await?;
        Ok(Response::new(profile(&user)))
    }

    async fn get_profile(
        &self,
        request: Request<GetProfileRequest>,
    ) -> Result<Response<Profile>, Status> {
        let rid = request_id(request.metadata());
        let user = self.existing(request.into_inner().user_id, &rid).await?;
        Ok(Response::new(profile(&user)))
    }

    async fn find_profile(
        &self,
        request: Request<FindProfileRequest>,
    ) -> Result<Response<Profile>, Status> {
        let rid = request_id(request.metadata());
        let name = request.into_inner().username;
        match self.app.find_user(&name).await {
            Some(user) => Ok(Response::new(profile(&user))),
            None => Err(failed(social_core::Error::UnknownName(name), &rid)),
        }
    }

    async fn list_profiles(
        &self,
        request: Request<ListProfilesRequest>,
    ) -> Result<Response<ProfileList>, Status> {
        let ask = request.into_inner();
        let limit = match ask.limit {
            0 => LIST_LIMIT,
            limit => limit.min(LIST_LIMIT),
        };
        let mut profiles = Vec::new();
        let mut next = ask.after;
        while profiles.len() < limit as usize {
            let Some(id) = next.checked_add(1) else {
                break;
            };
            let Some(user) = self.app.user(UserId(id)).await else {
                break;
            };
            profiles.push(profile(&user));
            next = id;
        }
        Ok(Response::new(ProfileList { profiles }))
    }

    async fn follow(&self, request: Request<PairRequest>) -> Result<Response<Changed>, Status> {
        let rid = request_id(request.metadata());
        let pair = request.into_inner();
        let (who, whom) = (UserId(pair.user_id), UserId(pair.target_id));
        let added = self.app.follow(who, whom).await;
        added.map(changed).map_err(|error| failed(error, &rid))
    }

    async fn mute(&self, request: Request<PairRequest>) -> Result<Response<Changed>, Status> {
        let rid = request_id(request.metadata());
        let pair = request.into_inner();
        let (who, whom) = (UserId(pair.user_id), UserId(pair.target_id));
        let added = self.app.mute(who, whom).await;
        added.map(changed).map_err(|error| failed(error, &rid))
    }

    async fn relations(
        &self,
        request: Request<RelationsRequest>,
    ) -> Result<Response<RelationsReply>, Status> {
        let rid = request_id(request.metadata());
        let user = self.existing(request.into_inner().user_id, &rid).await?;
        let relations = self.app.relations(user.id()).await;
        Ok(Response::new(RelationsReply {
            following: ids(&relations.following),
            followers: ids(&relations.followers),
            muted: ids(&relations.muted),
            audience: ids(&relations.audience),
        }))
    }

    async fn put_avatar(
        &self,
        request: Request<PutAvatarRequest>,
    ) -> Result<Response<PutAvatarReply>, Status> {
        let rid = request_id(request.metadata());
        let put = request.into_inner();
        let owner = self.existing(put.user_id, &rid).await?.id();
        if !put.content_type.starts_with("image/") {
            return Err(domain(
                "unsupported",
                "аватар это картинка: Content-Type image/...",
            ));
        }
        if put.data.len() > AVATAR_MAX {
            return Err(domain(
                "too_large",
                &format!("аватар больше {AVATAR_MAX} байт"),
            ));
        }
        let avatar = Avatar {
            content_type: put.content_type,
            bytes: Arc::new(put.data),
        };
        let created = self.avatars.put(owner, avatar);
        Ok(Response::new(PutAvatarReply { created }))
    }

    async fn get_avatar(
        &self,
        request: Request<AvatarRequest>,
    ) -> Result<Response<AvatarReply>, Status> {
        let owner = UserId(request.into_inner().user_id);
        match self.avatars.get(owner) {
            Some(avatar) => Ok(Response::new(AvatarReply {
                content_type: avatar.content_type,
                data: avatar.bytes.as_ref().clone(),
            })),
            None => Err(domain("not_found", &format!("у {owner} нет аватара"))),
        }
    }

    async fn delete_avatar(
        &self,
        request: Request<AvatarRequest>,
    ) -> Result<Response<Changed>, Status> {
        let owner = UserId(request.into_inner().user_id);
        Ok(changed(self.avatars.remove(owner)))
    }
}
