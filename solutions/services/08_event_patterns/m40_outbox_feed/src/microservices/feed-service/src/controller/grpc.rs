use std::sync::Arc;

use contracts::UserId;
use grpc_kit::caller_of;
use proto::invalid;
use proto::social::v1::feeds_server::Feeds;
use proto::social::v1::{FeedReply, FeedRequest};
use tonic::{Request, Response, Status};

use crate::dto;
use crate::service::FeedService;

pub struct FeedsController {
    service: Arc<FeedService>,
}

impl FeedsController {
    pub fn new(service: Arc<FeedService>) -> FeedsController {
        FeedsController { service }
    }
}

#[tonic::async_trait]
impl Feeds for FeedsController {
    async fn page(&self, request: Request<FeedRequest>) -> Result<Response<FeedReply>, Status> {
        let viewer = UserId(caller_of(&request)?);
        let (page, size) = dto::page(request.get_ref()).map_err(|errors| invalid(&errors))?;
        let items = self.service.page(viewer, page, size).await;
        Ok(Response::new(FeedReply {
            posts: items.iter().map(dto::post).collect(),
        }))
    }
}
