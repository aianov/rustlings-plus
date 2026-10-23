use contracts::ErrorCode;
use grpc_kit::Stamp;
use proto::from_status;
use proto::social::v1::CreateProfileRequest;
use proto::social::v1::users_client::UsersClient;
use tonic::transport::Channel;

use crate::common::Account;
use crate::repository::BoxFuture;
use crate::service::{ProfileDirectory, ProfileError};

pub struct GrpcProfiles {
    channel: Channel,
    stamp: Stamp,
}

impl GrpcProfiles {
    pub fn new(channel: Channel, stamp: Stamp) -> GrpcProfiles {
        GrpcProfiles { channel, stamp }
    }
}

impl ProfileDirectory for GrpcProfiles {
    fn create(&self, username: String) -> BoxFuture<'_, Result<Account, ProfileError>> {
        Box::pin(async move {
            let mut users = UsersClient::with_interceptor(self.channel.clone(), self.stamp.clone());
            match users
                .create_profile(CreateProfileRequest { username })
                .await
            {
                Ok(profile) => {
                    let profile = profile.into_inner();
                    Ok(Account::new(profile.id, &profile.username))
                }
                Err(status) => {
                    let body = from_status(&status, "сервис профилей");
                    match body.known() {
                        Some(code) if code.is_client_fault() => Err(ProfileError::Refused(body)),
                        Some(ErrorCode::Unavailable | ErrorCode::Timeout) => {
                            Err(ProfileError::Unavailable)
                        }
                        _ => Err(ProfileError::Failed(status.to_string())),
                    }
                }
            }
        })
    }
}
