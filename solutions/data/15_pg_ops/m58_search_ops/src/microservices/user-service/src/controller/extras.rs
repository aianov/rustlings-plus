use contracts::UserId;
use grpc_kit::caller_of;
use proto::request_id;
use proto::social::v1::{OwnProfileRequest, PeopleQuery, ProfileList, Settings as SettingsReply};
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
}
