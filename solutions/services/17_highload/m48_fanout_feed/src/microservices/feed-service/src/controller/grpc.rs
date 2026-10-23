use std::sync::Arc;

use contracts::UserId;
use grpc_kit::caller_of;
use proto::social::v1::feeds_server::Feeds;
use proto::social::v1::{FeedReply, FeedRequest};
use proto::{domain, invalid};
use tonic::{Request, Response, Status};

use crate::common::Cursor;
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
        let raw = request.get_ref().cursor.as_str();
        if raw.is_empty() && page > 1 {
            let items = self.service.page(viewer, page, size).await;
            return Ok(Response::new(FeedReply {
                posts: items.iter().map(dto::post).collect(),
                next: String::new(),
            }));
        }
        let before = if raw.is_empty() {
            None
        } else {
            Some(Cursor::decode(raw).map_err(|error| domain("validation", &error.to_string()))?)
        };
        let (items, next) = self.service.after(viewer, before, size).await;
        Ok(Response::new(FeedReply {
            posts: items.iter().map(dto::post).collect(),
            next: next.map(|cursor| cursor.encode()).unwrap_or_default(),
        }))
    }
}
