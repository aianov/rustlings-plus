use contracts::{ErrorBody, ErrorCode, Role, UserId, UserView};
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

    pub async fn check(
        &self,
        token: &str,
        stamp: &Stamp,
    ) -> Result<Option<(UserView, Role)>, ErrorBody> {
        let ask = ValidateTokenRequest {
            token: token.to_string(),
        };
        match self.upstreams.auth(stamp).validate_token(ask).await {
            Ok(account) => {
                let account = account.into_inner();
                let user = UserView::named(UserId(account.user_id), &account.username);
                Ok(Some((user, Role::parse(&account.role))))
            }
            Err(status) if status.code() == Code::Unauthenticated => {
                let body = failure(&status, "сервис входа", stamp.request_id());
                if body.known() == Some(ErrorCode::Expired) {
                    return Err(body);
                }
                Ok(None)
            }
            Err(status) => Err(failure(&status, "сервис входа", stamp.request_id())),
        }
    }
}
