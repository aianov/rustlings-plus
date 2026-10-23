use contracts::UserId;
use proto::social::v1::{ExportRequest, FeedReply, Post as PostReply, SearchRequest};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use super::{PostsController, failed};
use crate::dto;
use crate::service::SearchAsk;

fn filled(text: String) -> Option<String> {
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

impl PostsController {
    pub(super) async fn search_of(
        &self,
        request: Request<SearchRequest>,
    ) -> Result<Response<FeedReply>, Status> {
        let rid = proto::request_id(request.metadata());
        let ask = request.into_inner();
        let ask = SearchAsk {
            author: (ask.author_id > 0).then_some(UserId(ask.author_id)),
            tag: filled(ask.tag),
            text: filled(ask.text),
            since: (ask.since > 0).then_some(ask.since),
            sort: ask.sort,
            limit: ask.limit,
        };
        let found = self.service.search(ask).await;
        let found = found.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(FeedReply {
            posts: found.iter().map(dto::post).collect(),
            next: String::new(),
        }))
    }

    pub(super) async fn export_of(
        &self,
        request: Request<ExportRequest>,
    ) -> Result<Response<ReceiverStream<Result<PostReply, Status>>>, Status> {
        let (author, _, rid) = self.actor(&request)?;
        let mut posts = self
            .service
            .export(author)
            .map_err(|error| failed(&error, &rid))?;
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            while let Some(post) = posts.recv().await {
                if tx.send(Ok(dto::post(&post))).await.is_err() {
                    return;
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
