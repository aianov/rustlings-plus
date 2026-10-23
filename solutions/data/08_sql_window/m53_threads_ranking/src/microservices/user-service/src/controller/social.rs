use contracts::UserId;
use proto::social::v1::{ProfileList, ProfilesRequest, RelatedRequest, UserCounters};
use tonic::{Request, Response, Status};

use super::{MAX_BATCH, UsersController, failed};
use crate::dto;
use crate::service::Related;

impl UsersController {
    pub(super) async fn profiles_of(
        &self,
        request: Request<ProfilesRequest>,
    ) -> Result<Response<ProfileList>, Status> {
        let ids = request.into_inner().ids;
        if ids.len() > MAX_BATCH {
            return Err(Status::invalid_argument(format!(
                "не больше {MAX_BATCH} профилей за раз"
            )));
        }
        let mut profiles = Vec::with_capacity(ids.len());
        for id in ids {
            if let Ok(user) = self.service.profile(UserId(id)).await {
                let avatar = self.service.avatar_of(user.id).await;
                profiles.push(dto::with_avatar(dto::profile(&user), avatar));
            }
        }
        Ok(Response::new(ProfileList { profiles }))
    }

    pub(super) async fn list_related(
        &self,
        kind: Related,
        request: Request<RelatedRequest>,
    ) -> Result<Response<ProfileList>, Status> {
        let rid = proto::request_id(request.metadata());
        let ask = request.into_inner();
        let users = self
            .service
            .related(kind, UserId(ask.user_id), ask.after, ask.limit as usize)
            .await
            .map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(ProfileList {
            profiles: users.iter().map(dto::profile).collect(),
        }))
    }

    pub(super) async fn count_related(
        &self,
        request: Request<RelatedRequest>,
    ) -> Result<Response<UserCounters>, Status> {
        let rid = proto::request_id(request.metadata());
        let (followers, following) = self
            .service
            .counters(UserId(request.into_inner().user_id))
            .await
            .map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(UserCounters {
            followers,
            following,
        }))
    }
}
