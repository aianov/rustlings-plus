use contracts::UserId;
use grpc_kit::caller_of;
use proto::request_id;
use proto::social::v1::{
    GetProfileRequest, OwnProfileRequest, PeopleQuery, Profile, ProfileList,
    Settings as SettingsReply,
};
use tonic::{Request, Response, Status};

use super::{UsersController, failed};
use crate::common::Settings;
use crate::dto;

fn reply(settings: Settings) -> SettingsReply {
    SettingsReply {
        theme: settings.theme,
        language: settings.language,
        private: settings.private,
        digest: settings.digest,
    }
}

impl UsersController {
    pub(super) async fn people_of(
        &self,
        request: Request<PeopleQuery>,
    ) -> Result<Response<ProfileList>, Status> {
        let rid = request_id(request.metadata());
        let ask = request.into_inner();
        let found = self.service.people(&ask.query, ask.limit as usize).await;
        let found = found.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(ProfileList {
            profiles: found.iter().map(dto::profile).collect(),
        }))
    }

    pub(super) async fn settings_of(
        &self,
        request: Request<OwnProfileRequest>,
    ) -> Result<Response<SettingsReply>, Status> {
        let rid = request_id(request.metadata());
        let user = UserId(caller_of(&request)?);
        let settings = self.service.settings(user).await;
        let settings = settings.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(reply(settings)))
    }

    pub(super) async fn patch_of(
        &self,
        request: Request<SettingsReply>,
    ) -> Result<Response<SettingsReply>, Status> {
        let rid = request_id(request.metadata());
        let user = UserId(caller_of(&request)?);
        let ask = request.into_inner();
        let patch = Settings {
            theme: ask.theme,
            language: ask.language,
            private: ask.private,
            digest: ask.digest,
        };
        let saved = self.service.patch_settings(user, patch).await;
        let saved = saved.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(reply(saved)))
    }

    pub(super) async fn profile_of(
        &self,
        request: Request<GetProfileRequest>,
    ) -> Result<Response<Profile>, Status> {
        let rid = request_id(request.metadata());
        let viewer = caller_of(&request).ok().map(UserId);
        let id = UserId(request.into_inner().user_id);
        let user = self.service.profile(id).await;
        let user = user.map_err(|error| failed(&error, &rid))?;
        self.service.visited(user.id, viewer).await;
        let avatar = self.service.avatar_of(user.id).await;
        Ok(Response::new(dto::with_avatar(dto::profile(&user), avatar)))
    }

    pub(super) async fn guests_list(
        &self,
        request: Request<OwnProfileRequest>,
    ) -> Result<Response<ProfileList>, Status> {
        let rid = request_id(request.metadata());
        let owner = UserId(caller_of(&request)?);
        let guests = self.service.guests_of(owner).await;
        let guests = guests.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(ProfileList {
            profiles: guests.iter().map(dto::profile).collect(),
        }))
    }
}
