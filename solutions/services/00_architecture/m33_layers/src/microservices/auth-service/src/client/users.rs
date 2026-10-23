use grpc_kit::Stamp;
use proto::social::v1::CreateProfileRequest;
use proto::social::v1::users_client::UsersClient;
use proto::{domain_code, explain};
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
                    Ok(Account {
                        user_id: profile.id,
                        username: profile.username,
                    })
                }
                Err(status) => {
                    let (code, message) = explain(&status, "сервис профилей");
                    if domain_code(&status).is_some() {
                        return Err(ProfileError::Refused { code, message });
                    }
                    match code.as_str() {
                        "unavailable" => Err(ProfileError::Unavailable),
                        _ => Err(ProfileError::Failed(status.to_string())),
                    }
                }
            }
        })
    }
}
