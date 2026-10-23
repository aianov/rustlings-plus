use std::sync::Arc;

use contracts::{CommentId, Role};
use grpc_kit::role_of;
use proto::invalid;
use proto::social::v1::{Changed, CommentAction, DraftAction, DraftReply, DraftRequest};
use tonic::{Request, Response, Status};
use validation::Validator;

use super::{PostsController, failed, finish, origin};
use tokio::task::JoinSet;

use crate::client::Previewer;
use crate::common::{Actor, Draft, PostText};

impl PostsController {
    pub(super) async fn remove_comment(
        &self,
        request: Request<CommentAction>,
    ) -> Result<Response<Changed>, Status> {
        let (user, _, rid) = self.actor(&request)?;
        let origin = origin(&request);
        let actor = Actor {
            id: user,
            role: Role::parse(role_of(&request)),
        };
        let comment = CommentId(request.into_inner().comment_id);
        let service = Arc::clone(&self.service);
        let removing = async move { service.delete_comment(actor, comment, &origin).await };
        let changed = finish(removing, &rid).await?;
        Ok(Response::new(Changed { changed }))
    }

    pub(super) async fn keep_draft(
        &self,
        request: Request<DraftRequest>,
    ) -> Result<Response<DraftReply>, Status> {
        let (owner, _, rid) = self.actor(&request)?;
        let origin = origin(&request);
        let raw = request.into_inner().text;
        let mut check = Validator::new();
        let text = check.parse::<PostText>("text", &raw);
        let text = check.finish_with(text).map_err(|errors| invalid(&errors))?;
        let service = Arc::clone(&self.service);
        let kept = text.as_ref().to_string();
        let saving = async move { service.save_draft(owner, text, &origin).await };
        let draft_id = finish(saving, &rid).await?;
        Ok(Response::new(DraftReply {
            draft_id,
            text: kept,
        }))
    }

    pub(super) async fn read_draft(
        &self,
        request: Request<DraftAction>,
    ) -> Result<Response<DraftReply>, Status> {
        let (owner, _, rid) = self.actor(&request)?;
        let draft_id = request.into_inner().draft_id;
        let text = self.service.draft(owner, draft_id).await;
        let text = text.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(DraftReply { draft_id, text }))
    }

    pub(super) async fn drop_draft(
        &self,
        request: Request<DraftAction>,
    ) -> Result<Response<Changed>, Status> {
        let (owner, _, rid) = self.actor(&request)?;
        let origin = origin(&request);
        let draft_id = request.into_inner().draft_id;
        let service = Arc::clone(&self.service);
        let dropping = async move { service.delete_draft(owner, draft_id, &origin).await };
        let changed = finish(dropping, &rid).await?;
        Ok(Response::new(Changed { changed }))
    }
}

impl PostsController {
    pub fn with_previews(self, previews: Arc<Previewer>) -> PostsController {
        PostsController {
            previews: Some(previews),
            ..self
        }
    }

    pub(super) async fn previewed(&self, mut draft: Draft) -> Draft {
        let Some(previews) = &self.previews else {
            return draft;
        };
        let mut tasks = JoinSet::new();
        for (at, link) in draft.links.iter().enumerate() {
            if link.title.is_none() {
                let previews = Arc::clone(previews);
                let url = link.url.clone();
                tasks.spawn(async move { (at, previews.title(&url).await) });
            }
        }
        while let Some(done) = tasks.join_next().await {
            if let Ok((at, Some(title))) = done {
                if let Some(link) = draft.links.get_mut(at) {
                    link.title = Some(title);
                }
            }
        }
        draft
    }
}
