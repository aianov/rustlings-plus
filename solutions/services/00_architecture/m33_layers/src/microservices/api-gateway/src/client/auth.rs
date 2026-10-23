use contracts::{ErrorBody, UserId, UserView};
use grpc_kit::Stamp;
use proto::social::v1::ValidateTokenRequest;
use tonic::Code;

use crate::client::{Upstreams, failure};

#[derive(Clone)]
pub struct RemoteAuth {
    upstreams: Upstreams,
}

impl RemoteAuth {
    pub fn new(upstreams: Upstreams) -> RemoteAuth {
        RemoteAuth { upstreams }
    }

    pub async fn check(&self, token: &str, stamp: &Stamp) -> Result<Option<UserView>, ErrorBody> {
        let ask = ValidateTokenRequest {
            token: token.to_string(),
        };
        match self.upstreams.auth(stamp).validate_token(ask).await {
            Ok(account) => {
                let account = account.into_inner();
                Ok(Some(UserView {
                    id: UserId(account.user_id),
                    username: account.username,
                }))
            }
            Err(status) if status.code() == Code::Unauthenticated => Ok(None),
            Err(status) => Err(failure(&status, "сервис входа", stamp.request_id())),
        }
    }
}
