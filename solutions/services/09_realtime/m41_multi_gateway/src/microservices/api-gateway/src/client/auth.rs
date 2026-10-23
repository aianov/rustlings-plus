use contracts::{ErrorBody, ErrorCode, Role, UserId};
use grpc_kit::Stamp;
use proto::social::v1::ValidateTokenRequest;
use tonic::Code;

use crate::client::{Upstreams, failure};
use crate::common::{Principal, unix_now};

#[derive(Clone)]
pub struct RemoteAuth {
    upstreams: Upstreams,
}

impl RemoteAuth {
    pub fn new(upstreams: Upstreams) -> RemoteAuth {
        RemoteAuth { upstreams }
    }

    pub async fn check(&self, token: &str, stamp: &Stamp) -> Result<Option<Principal>, ErrorBody> {
        let ask = ValidateTokenRequest {
            token: token.to_string(),
        };
        match self.upstreams.auth(stamp).validate_token(ask).await {
            Ok(account) => {
                let account = account.into_inner();
                let principal = Principal {
                    id: UserId(account.user_id),
                    username: account.username,
                    role: Role::parse(&account.role),
                    session: account.session,
                    expires_at: account.expires_at,
                };
                if principal.is_expired(unix_now()) {
                    let expired = ErrorBody::public(ErrorCode::Expired);
                    return Err(expired.with_request_id(stamp.request_id()));
                }
                Ok(Some(principal))
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
