use contracts::PostId;
use proto::request_id;
use proto::social::v1::{ThreadReply, ThreadRequest, TopReply, TopRequest};
use tonic::{Request, Response, Status};

use super::{PostsController, failed};
use crate::dto;

impl PostsController {
    pub(super) async fn thread_of(
        &self,
        request: Request<ThreadRequest>,
    ) -> Result<Response<ThreadReply>, Status> {
        let rid = request_id(request.metadata());
        let ask = request.into_inner();
        let nodes = self
            .service
            .thread(PostId(ask.post_id), ask.max_depth)
            .await;
        let nodes = nodes.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(ThreadReply {
            nodes: nodes.into_iter().map(dto::node).collect(),
        }))
    }

    pub(super) async fn top_of(
        &self,
        request: Request<TopRequest>,
    ) -> Result<Response<TopReply>, Status> {
        let rid = request_id(request.metadata());
        let top = self.service.top(request.into_inner().limit).await;
        let top = top.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(TopReply {
            authors: top.into_iter().map(dto::rank).collect(),
        }))
    }
}
