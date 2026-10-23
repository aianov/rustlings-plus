use contracts::UserId;
use grpc_kit::Stamp;
use proto::domain_code;
use proto::social::v1::users_client::UsersClient;
use proto::social::v1::{GetProfileRequest, RelationsRequest};
use tonic::transport::Channel;
use tonic::{Code, Status};

use crate::common::{Author, Graph};
use crate::repository::BoxFuture;
use crate::service::{DirectoryError, UserDirectory};

#[derive(Clone)]
pub struct GrpcUsers {
    channel: Channel,
    stamp: Stamp,
}

fn missing_or(status: Status) -> Result<(), DirectoryError> {
    if domain_code(&status) == Some("not_found") {
        return Ok(());
    }
    match status.code() {
        Code::Unavailable | Code::DeadlineExceeded | Code::Cancelled => {
            Err(DirectoryError::Unavailable)
        }
        _ => Err(DirectoryError::Failed(status.to_string())),
    }
}

impl GrpcUsers {
    pub fn new(channel: Channel, stamp: Stamp) -> GrpcUsers {
        GrpcUsers { channel, stamp }
    }

    fn client(
        &self,
    ) -> UsersClient<tonic::service::interceptor::InterceptedService<Channel, Stamp>> {
        UsersClient::with_interceptor(self.channel.clone(), self.stamp.clone())
    }
}

impl UserDirectory for GrpcUsers {
    fn author(&self, id: UserId) -> BoxFuture<'_, Result<Option<Author>, DirectoryError>> {
        Box::pin(async move {
            match self
                .client()
                .get_profile(GetProfileRequest { user_id: id.0 })
                .await
            {
                Ok(profile) => {
                    let profile = profile.into_inner();
                    Ok(Some(Author {
                        id: UserId(profile.id),
                        name: profile.username,
                    }))
                }
                Err(status) => missing_or(status).map(|()| None),
            }
        })
    }

    fn graph(&self, viewer: UserId) -> BoxFuture<'_, Result<Option<Graph>, DirectoryError>> {
        Box::pin(async move {
            match self
                .client()
                .relations(RelationsRequest { user_id: viewer.0 })
                .await
            {
                Ok(reply) => {
                    let reply = reply.into_inner();
                    Ok(Some(Graph {
                        following: reply.following.into_iter().map(UserId).collect(),
                        muted: reply.muted.into_iter().map(UserId).collect(),
                    }))
                }
                Err(status) => missing_or(status).map(|()| None),
            }
        })
    }
}
