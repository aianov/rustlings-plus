mod audience;
mod courier;

use std::sync::Arc;

use contracts::{NoticeKind, UserId};

use crate::common::{Draft, Incoming, Notice, Trigger};
use crate::repository::{NoticeRepository, Recorded};

pub use audience::{Audience, AudienceError};
pub use courier::{Courier, Quiet};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Handled {
    pub stored: Vec<Notice>,
    pub duplicates: usize,
}

pub struct NotifyService {
    notices: Arc<dyn NoticeRepository>,
    audience: Arc<dyn Audience>,
    courier: Arc<dyn Courier>,
}

fn direct(incoming: &Incoming, kind: NoticeKind) -> Vec<Draft> {
    vec![Draft {
        to: incoming.owner,
        kind,
        actor: incoming.actor,
        post: incoming.post,
        count: None,
    }]
}

impl NotifyService {
    pub fn new(
        notices: Arc<dyn NoticeRepository>,
        audience: Arc<dyn Audience>,
        courier: Arc<dyn Courier>,
    ) -> NotifyService {
        NotifyService {
            notices,
            audience,
            courier,
        }
    }

    pub async fn handle(&self, incoming: Incoming) -> Result<Handled, AudienceError> {
        let drafts = self.drafts(&incoming).await?;
        let mut handled = Handled::default();
        for draft in drafts {
            match self.notices.record(draft).await {
                Recorded::Stored(notice) => {
                    let request_id = incoming.request_id.clone();
                    self.courier.deliver(notice.clone(), request_id).await;
                    handled.stored.push(notice);
                }
                Recorded::Duplicate => handled.duplicates += 1,
            }
        }
        Ok(handled)
    }

    async fn drafts(&self, incoming: &Incoming) -> Result<Vec<Draft>, AudienceError> {
        match incoming.kind {
            Trigger::Followed => Ok(vec![Draft {
                to: incoming.owner,
                kind: NoticeKind::Followed,
                actor: incoming.actor,
                post: None,
                count: None,
            }]),
            Trigger::Liked => Ok(direct(incoming, NoticeKind::Liked)),
            Trigger::Commented => Ok(direct(incoming, NoticeKind::Commented)),
            Trigger::Posted => {
                let request_id = incoming.request_id.as_deref().unwrap_or("-");
                let readers = self.audience.readers(incoming.owner, request_id).await?;
                Ok(readers
                    .into_iter()
                    .filter(|reader| *reader != incoming.owner)
                    .map(|to| Draft {
                        to,
                        kind: NoticeKind::NewPost,
                        actor: incoming.owner,
                        post: incoming.post,
                        count: None,
                    })
                    .collect())
            }
        }
    }

    pub async fn unread(&self, user: UserId) -> u32 {
        self.notices.unread(user).await
    }

    pub async fn list(&self, user: UserId, limit: usize) -> Vec<Notice> {
        self.notices.list(user, limit).await
    }

    pub async fn mark_read(&self, user: UserId) -> u32 {
        self.notices.mark_read(user).await
    }

    pub async fn digest_targets(&self) -> Vec<(UserId, u32)> {
        self.notices.unread_counts().await
    }

    pub async fn send_digest(&self, user: UserId, unread: u32, request_id: &str) -> bool {
        let draft = Draft {
            to: user,
            kind: NoticeKind::Digest,
            actor: user,
            post: None,
            count: Some(unread),
        };
        match self.notices.record(draft).await {
            Recorded::Stored(notice) => {
                self.courier
                    .deliver(notice, Some(request_id.to_string()))
                    .await;
                true
            }
            Recorded::Duplicate => false,
        }
    }
}
