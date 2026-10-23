use contracts::UserId;
use grpc_kit::{Stamp, caller_of};
use proto::social::v1::{FeedEvent, WatchRequest};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use super::{PostsController, RECENT_MAX, SYNC_BUDGET, failed};
use crate::client::GrpcUsers;
use crate::dto;

impl PostsController {
    pub(super) async fn stream(
        &self,
        request: Request<WatchRequest>,
    ) -> Result<Response<ReceiverStream<Result<FeedEvent, Status>>>, Status> {
        let viewer = UserId(caller_of(&request)?);
        let stamp = Stamp::follow(&request).within(SYNC_BUDGET);
        let rid = stamp.request_id().to_string();
        let users = GrpcUsers::new(self.users.clone(), stamp);
        let recent = match request.into_inner().recent {
            0 => 20,
            recent => recent.min(RECENT_MAX),
        };
        let watch = self.service.watch(&users, viewer, recent as usize).await;
        let mut watch = watch.map_err(|error| failed(&error, &rid))?;
        let (tx, rx) = mpsc::channel(16);
        let stop = self.stop.clone();
        tokio::spawn(async move {
            let mut events: Vec<FeedEvent> = watch.history().iter().map(dto::history).collect();
            events.push(dto::caught_up());
            for event in events {
                if tx.send(Ok(event)).await.is_err() {
                    return;
                }
            }
            loop {
                let next = tokio::select! {
                    next = watch.next() => next,
                    () = tx.closed() => return,
                    () = stop.cancelled() => return,
                };
                let Some(post) = next else {
                    return;
                };
                if tx.send(Ok(dto::live(&post))).await.is_err() {
                    return;
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
