use std::future::Future;
use std::sync::Arc;

use contracts::{ErrorBody, ErrorCode, Origin, UserId};
use grpc_kit::{Stamp, caller_of};
use proto::social::v1::users_server::Users;
use proto::social::v1::{
    Avatar as AvatarReply, AvatarRequest, Changed, CreateProfileRequest, DeleteAvatarRequest,
    FindProfileRequest, GetProfileRequest, ListProfilesRequest, PairRequest, Profile, ProfileList,
    PutAvatarReply, PutAvatarRequest, RelationsReply, RelationsRequest, UpdateProfileRequest,
};
use proto::{internal, invalid, request_id, to_status};
use tonic::{Request, Response, Status};

use crate::dto;
use crate::service::{UserError, UserService};

pub struct UsersController {
    service: Arc<UserService>,
}

fn failed(error: &UserError, rid: &str) -> Status {
    if error.code() == ErrorCode::Internal {
        eprintln!("user-service [{rid}]: {}", service_runtime::report(error));
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
            eprintln!("user-service [{rid}]: задача записи упала");
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
}

#[tonic::async_trait]
impl Users for UsersController {
    async fn create_profile(
        &self,
        request: Request<CreateProfileRequest>,
    ) -> Result<Response<Profile>, Status> {
        let rid = request_id(request.metadata());
        let name = dto::new_profile(request.get_ref()).map_err(|errors| invalid(&errors))?;
        let service = Arc::clone(&self.service);
        let user = finish(async move { service.register(name).await }, &rid).await?;
        Ok(Response::new(dto::profile(&user)))
    }

    async fn update_profile(
        &self,
        request: Request<UpdateProfileRequest>,
    ) -> Result<Response<Profile>, Status> {
        let rid = request_id(request.metadata());
        let who = UserId(caller_of(&request)?);
        let change = dto::profile_change(request.get_ref()).map_err(|errors| invalid(&errors))?;
        let service = Arc::clone(&self.service);
        let user = finish(async move { service.update(who, change).await }, &rid).await?;
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
        Ok(Response::new(dto::profile(&user)))
    }

    async fn find_profile(
        &self,
        request: Request<FindProfileRequest>,
    ) -> Result<Response<Profile>, Status> {
        let rid = request_id(request.metadata());
        let name = request.into_inner().username;
        let user = self.service.find(&name).await;
        let user = user.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(dto::profile(&user)))
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
        let who = UserId(caller_of(&request)?);
        let whom = UserId(request.into_inner().target_id);
        let service = Arc::clone(&self.service);
        let added = finish(async move { service.mute(who, whom).await }, &rid).await?;
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

    async fn put_avatar(
        &self,
        request: Request<PutAvatarRequest>,
    ) -> Result<Response<PutAvatarReply>, Status> {
        let rid = request_id(request.metadata());
        let owner = UserId(caller_of(&request)?);
        let put = request.into_inner();
        let stored = self
            .service
            .put_avatar(owner, put.content_type, put.data)
            .await;
        let created = stored.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(PutAvatarReply { created }))
    }

    async fn get_avatar(
        &self,
        request: Request<AvatarRequest>,
    ) -> Result<Response<AvatarReply>, Status> {
        let rid = request_id(request.metadata());
        let owner = UserId(request.into_inner().user_id);
        let avatar = self.service.avatar(owner).await;
        let avatar = avatar.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(dto::avatar(avatar)))
    }

    async fn delete_avatar(
        &self,
        request: Request<DeleteAvatarRequest>,
    ) -> Result<Response<Changed>, Status> {
        let owner = UserId(caller_of(&request)?);
        Ok(changed(self.service.delete_avatar(owner).await))
    }
}
