use contracts::{ErrorBody, UserId, UserView};
use proto::social::v1::ValidateTokenRequest;
use proto::social::v1::auth_client::AuthClient;
use proto::traced;
use tonic::Code;
use tonic::transport::Channel;

use crate::upstream::failure;

#[derive(Clone)]
pub struct RemoteAuth {
    client: AuthClient<Channel>,
}

impl RemoteAuth {
    pub fn new(client: AuthClient<Channel>) -> RemoteAuth {
        RemoteAuth { client }
    }

    pub async fn check(&self, token: &str, rid: &str) -> Result<Option<UserView>, ErrorBody> {
        let ask = ValidateTokenRequest {
            token: token.to_string(),
        };
        match self.client.clone().validate_token(traced(ask, rid)).await {
            Ok(account) => {
                let account = account.into_inner();
                Ok(Some(UserView {
                    id: UserId(account.user_id),
                    username: account.username,
                }))
            }
            Err(status) if status.code() == Code::Unauthenticated => Ok(None),
            Err(status) => Err(failure(&status, "сервис входа", rid)),
        }
    }
}
