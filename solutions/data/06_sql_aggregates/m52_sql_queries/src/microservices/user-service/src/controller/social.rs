use contracts::UserId;
use proto::social::v1::{ProfileList, RelatedRequest, UserCounters};
use tonic::{Request, Response, Status};

use super::{UsersController, failed};
use crate::dto;
use crate::service::Related;

impl UsersController {
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
