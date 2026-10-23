mod extras;
mod files;
mod social;

use std::future::Future;
use std::sync::Arc;

use contracts::{ErrorBody, ErrorCode, Origin, UserId};
use grpc_kit::{Stamp, caller_of};
use proto::social::v1::users_server::Users;
use proto::social::v1::{
    Avatar as AvatarReply, AvatarChunk, AvatarRequest, Changed, CreateProfileRequest,
    DeleteAvatarRequest, FileRequest, FindProfileRequest, GetProfileRequest, ListProfilesRequest,
    OwnProfileRequest, PairRequest, PeopleQuery, Profile, ProfileList, ProfilesRequest,
    PutAvatarReply, RelatedRequest, RelationsReply, RelationsRequest, Settings as SettingsReply,
    UpdateProfileRequest, UserCounters,
};
use proto::{internal, invalid, request_id, to_status};
use tonic::{Request, Response, Status, Streaming};

use crate::dto;
use crate::service::{Fate, Related, UserError, UserService};

pub const MAX_BATCH: usize = 100;

pub struct UsersController {
    service: Arc<UserService>,
}

fn failed(error: &UserError, rid: &str) -> Status {
    if error.code() == ErrorCode::Internal {
        tracing::warn!("user-service [{rid}]: {}", service_runtime::report(error));
    }
    to_status(&ErrorBody::new(error.code(), error.to_string()))
}

fn origin<T>(request: &Request<T>) -> Origin {
    let stamp = Stamp::follow(request);
    Origin::new(stamp.request_id()).traced(stamp.trace_id())
}

async fn finish<T, F>(work: F, rid: &str) -> Result<T, Status>
where
    T: Send + 'static,
    F: Future<Output = Result<T, UserError>> + Send + 'static,
{
    match tokio::spawn(work).await {
        Ok(done) => done.map_err(|error| failed(&error, rid)),
        Err(..) => {
            tracing::warn!("user-service [{rid}]: задача записи упала");
            Err(internal())
        }
    }
}

fn changed(changed: bool) -> Response<Changed> {
    Response::new(Changed { changed })
}

impl UsersController {
    pub fn new(service: Arc<UserService>) -> UsersController {
        UsersController { service }
    }

    async fn settle(
        &self,
        request: Request<OwnProfileRequest>,
        fate: Fate,
    ) -> Result<Response<Changed>, Status> {
        let who = UserId(caller_of(&request)?);
        let (origin, rid) = (origin(&request), request_id(request.metadata()));
        let service = Arc::clone(&self.service);
        let settled = finish(
            async move { service.settle(who, fate, &origin).await },
            &rid,
        );
        settled.await.map(changed)
    }
}

#[tonic::async_trait]
impl Users for UsersController {
    async fn create_profile(
        &self,
        request: Request<CreateProfileRequest>,
    ) -> Result<Response<Profile>, Status> {
        let rid = request_id(request.metadata());
        let origin = origin(&request);
        let name = dto::new_profile(request.get_ref()).map_err(|errors| invalid(&errors))?;
        let service = Arc::clone(&self.service);
        let user = finish(async move { service.register(name, &origin).await }, &rid).await?;
        Ok(Response::new(dto::profile(&user)))
    }

    async fn update_profile(
        &self,
        request: Request<UpdateProfileRequest>,
    ) -> Result<Response<Profile>, Status> {
        let rid = request_id(request.metadata());
        let origin = origin(&request);
        let who = UserId(caller_of(&request)?);
        let change = dto::profile_change(request.get_ref()).map_err(|errors| invalid(&errors))?;
        let service = Arc::clone(&self.service);
        let updating = async move { service.update(who, change, &origin).await };
        let user = finish(updating, &rid).await?;
        Ok(Response::new(dto::profile(&user)))
    }

    async fn get_profile(
        &self,
        request: Request<GetProfileRequest>,
    ) -> Result<Response<Profile>, Status> {
        let rid = request_id(request.metadata());
        let id = UserId(request.into_inner().user_id);
        let user = self.service.profile(id).await;
        let user = user.map_err(|error| failed(&error, &rid))?;
        let avatar = self.service.avatar_of(user.id).await;
        Ok(Response::new(dto::with_avatar(dto::profile(&user), avatar)))
    }

    async fn get_profiles(
        &self,
        request: Request<ProfilesRequest>,
    ) -> Result<Response<ProfileList>, Status> {
        self.profiles_of(request).await
    }

    async fn suggest(
        &self,
        request: Request<RelatedRequest>,
    ) -> Result<Response<ProfileList>, Status> {
        self.list_related(Related::Suggested, request).await
    }

    async fn followers(
        &self,
        request: Request<RelatedRequest>,
    ) -> Result<Response<ProfileList>, Status> {
        self.list_related(Related::Followers, request).await
    }

    async fn following(
        &self,
        request: Request<RelatedRequest>,
    ) -> Result<Response<ProfileList>, Status> {
        self.list_related(Related::Following, request).await
    }

    async fn friends(
        &self,
        request: Request<RelatedRequest>,
    ) -> Result<Response<ProfileList>, Status> {
        self.list_related(Related::Friends, request).await
    }

    async fn counters(
        &self,
        request: Request<RelatedRequest>,
    ) -> Result<Response<UserCounters>, Status> {
        self.count_related(request).await
    }

    async fn find_profile(
        &self,
        request: Request<FindProfileRequest>,
    ) -> Result<Response<Profile>, Status> {
        let rid = request_id(request.metadata());
        let name = request.into_inner().username;
        let user = self.service.find(&name).await;
        let user = user.map_err(|error| failed(&error, &rid))?;
        let avatar = self.service.avatar_of(user.id).await;
        Ok(Response::new(dto::with_avatar(dto::profile(&user), avatar)))
    }

    async fn list_profiles(
        &self,
        request: Request<ListProfilesRequest>,
    ) -> Result<Response<ProfileList>, Status> {
        let ask = request.into_inner();
        let users = self.service.list(ask.after, ask.limit as usize).await;
        let profiles = users.iter().map(dto::profile).collect();
        Ok(Response::new(ProfileList { profiles }))
    }

    async fn follow(&self, request: Request<PairRequest>) -> Result<Response<Changed>, Status> {
        let rid = request_id(request.metadata());
        let origin = origin(&request);
        let who = UserId(caller_of(&request)?);
        let whom = UserId(request.into_inner().target_id);
        let service = Arc::clone(&self.service);
        let following = async move { service.follow(who, whom, &origin).await };
        let added = finish(following, &rid).await?;
        Ok(changed(added))
    }

    async fn mute(&self, request: Request<PairRequest>) -> Result<Response<Changed>, Status> {
        let rid = request_id(request.metadata());
        let origin = origin(&request);
        let who = UserId(caller_of(&request)?);
        let whom = UserId(request.into_inner().target_id);
        let service = Arc::clone(&self.service);
        let added = finish(async move { service.mute(who, whom, &origin).await }, &rid).await?;
        Ok(changed(added))
    }

    async fn relations(
        &self,
        request: Request<RelationsRequest>,
    ) -> Result<Response<RelationsReply>, Status> {
        let rid = request_id(request.metadata());
        let id = UserId(request.into_inner().user_id);
        let relations = self.service.relations(id).await;
        let relations = relations.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(dto::relations(&relations)))
    }

    async fn upload_avatar(
        &self,
        request: Request<Streaming<AvatarChunk>>,
    ) -> Result<Response<PutAvatarReply>, Status> {
        self.upload_of(request).await
    }

    async fn get_avatar(
        &self,
        request: Request<AvatarRequest>,
    ) -> Result<Response<AvatarReply>, Status> {
        let rid = request_id(request.metadata());
        let owner = UserId(request.into_inner().user_id);
        let avatar = self.service.avatar(owner).await;
        let (avatar, data) = avatar.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(dto::avatar(avatar, data)))
    }

    async fn get_file(
        &self,
        request: Request<FileRequest>,
    ) -> Result<Response<AvatarReply>, Status> {
        self.file_of(request).await
    }

    async fn delete_avatar(
        &self,
        request: Request<DeleteAvatarRequest>,
    ) -> Result<Response<Changed>, Status> {
        let owner = UserId(caller_of(&request)?);
        Ok(changed(self.service.delete_avatar(owner).await))
    }

    async fn hide_profile(
        &self,
        request: Request<OwnProfileRequest>,
    ) -> Result<Response<Changed>, Status> {
        self.settle(request, Fate::Hide).await
    }

    async fn show_profile(
        &self,
        request: Request<OwnProfileRequest>,
    ) -> Result<Response<Changed>, Status> {
        self.settle(request, Fate::Show).await
    }

    async fn delete_profile(
        &self,
        request: Request<OwnProfileRequest>,
    ) -> Result<Response<Changed>, Status> {
        self.settle(request, Fate::Erase).await
    }

    async fn search_people(
        &self,
        request: Request<PeopleQuery>,
    ) -> Result<Response<ProfileList>, Status> {
        self.people_of(request).await
    }

    async fn get_settings(
        &self,
        request: Request<OwnProfileRequest>,
    ) -> Result<Response<SettingsReply>, Status> {
        self.settings_of(request).await
    }

    async fn update_settings(
        &self,
        request: Request<SettingsReply>,
    ) -> Result<Response<SettingsReply>, Status> {
        self.patch_of(request).await
    }
}
