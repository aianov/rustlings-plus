use contracts::UserId;
use grpc_kit::Stamp;
use proto::domain_code;
use proto::social::v1::GetProfileRequest;
use proto::social::v1::users_client::UsersClient;
use tonic::Code;
use tonic::transport::Channel;

use crate::repository::BoxFuture;
use crate::service::{DirectoryError, UserDirectory};

#[derive(Clone)]
pub struct GrpcUsers {
    channel: Channel,
    stamp: Stamp,
}

impl GrpcUsers {
    pub fn new(channel: Channel, stamp: Stamp) -> GrpcUsers {
        GrpcUsers { channel, stamp }
    }
}

impl UserDirectory for GrpcUsers {
    fn exists(&self, id: UserId) -> BoxFuture<'_, Result<bool, DirectoryError>> {
        Box::pin(async move {
            let mut users = UsersClient::with_interceptor(self.channel.clone(), self.stamp.clone());
            match users.get_profile(GetProfileRequest { user_id: id.0 }).await {
                Ok(..) => Ok(true),
                Err(status) if domain_code(&status) == Some("not_found") => Ok(false),
                Err(status) => match status.code() {
                    Code::Unavailable | Code::DeadlineExceeded | Code::Cancelled => {
                        Err(DirectoryError::Unavailable)
                    }
                    _ => Err(DirectoryError::Failed(status.to_string())),
                },
            }
        })
    }
}
