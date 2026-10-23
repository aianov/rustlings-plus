use std::time::Duration;

use contracts::UserId;
use proto::social::v1::RelationsRequest;
use proto::social::v1::users_client::UsersClient;
use proto::traced;
use tokio::time::timeout;
use tonic::Code;
use tonic::transport::Channel;

use crate::repository::BoxFuture;
use crate::service::{Audience, AudienceError};

pub struct GrpcAudience {
    channel: Channel,
    limit: Duration,
}

impl GrpcAudience {
    pub fn new(channel: Channel, limit: Duration) -> GrpcAudience {
        GrpcAudience { channel, limit }
    }
}

impl Audience for GrpcAudience {
    fn readers(
        &self,
        author: UserId,
        request_id: &str,
    ) -> BoxFuture<'_, Result<Vec<UserId>, AudienceError>> {
        let ask = traced(RelationsRequest { user_id: author.0 }, request_id);
        Box::pin(async move {
            let mut users = UsersClient::new(self.channel.clone());
            match timeout(self.limit, users.relations(ask)).await {
                Err(..) => Err(AudienceError::Unavailable),
                Ok(Ok(reply)) => Ok(reply
                    .into_inner()
                    .audience
                    .into_iter()
                    .map(UserId)
                    .collect()),
                Ok(Err(status)) => match status.code() {
                    Code::Unavailable | Code::DeadlineExceeded | Code::Cancelled => {
                        Err(AudienceError::Unavailable)
                    }
                    _ => Err(AudienceError::Failed(status.message().to_string())),
                },
            }
        })
    }
}
