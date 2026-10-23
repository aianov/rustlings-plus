use std::sync::Arc;

use jobs::{Checkpoints, Failure, Saga, SagaError, SagaLog, Step, StepFuture, run_checkpointed};

use super::{AuthError, AuthService};
use crate::repository::BoxFuture;

pub const DELETE_KIND: &str = "delete_account";

pub const POSTS_PAGE: u32 = 50;

pub trait ProfileKeeper: Send + Sync {
    fn hide(&self, user_id: u64) -> BoxFuture<'_, Result<(), Failure>>;
    fn show(&self, user_id: u64) -> BoxFuture<'_, Result<(), Failure>>;
    fn erase(&self, user_id: u64) -> BoxFuture<'_, Result<(), Failure>>;
}

pub trait PostEraser: Send + Sync {
    fn page(&self, user_id: u64, limit: u32) -> BoxFuture<'_, Result<Vec<u64>, Failure>>;
    fn delete(&self, user_id: u64, post_id: u64) -> BoxFuture<'_, Result<(), Failure>>;
}

pub trait DeletionQueue: Send + Sync {
    fn request(&self, saga: String, user_id: u64) -> BoxFuture<'_, Result<(), Failure>>;
}

pub struct Deletion {
    pub profiles: Arc<dyn ProfileKeeper>,
    pub posts: Arc<dyn PostEraser>,
    pub log: SagaLog,
    pub checkpoints: Checkpoints,
}

fn storage(error: AuthError) -> Failure {
    Failure::Transient(error.to_string())
}

struct Freeze<'a> {
    auth: &'a AuthService,
    user: u64,
}

impl Step for Freeze<'_> {
    fn name(&self) -> &str {
        "freeze"
    }

    fn forward_only(&self) -> bool {
        false
    }

    fn run(&self) -> StepFuture<'_> {
        Box::pin(async { self.auth.freeze(self.user, true).await.map_err(storage) })
    }

    fn compensate(&self) -> StepFuture<'_> {
        Box::pin(async { self.auth.freeze(self.user, false).await.map_err(storage) })
    }
}

struct Hide<'a> {
    profiles: &'a dyn ProfileKeeper,
    user: u64,
}

impl Step for Hide<'_> {
    fn name(&self) -> &str {
        "hide"
    }

    fn forward_only(&self) -> bool {
        false
    }

    fn run(&self) -> StepFuture<'_> {
        self.profiles.hide(self.user)
    }

    fn compensate(&self) -> StepFuture<'_> {
        self.profiles.show(self.user)
    }
}

struct Posts<'a> {
    posts: &'a dyn PostEraser,
    checkpoints: &'a Checkpoints,
    saga: String,
    user: u64,
}

impl Step for Posts<'_> {
    fn name(&self) -> &str {
        "posts"
    }

    fn forward_only(&self) -> bool {
        true
    }

    fn run(&self) -> StepFuture<'_> {
        Box::pin(async {
            let job = format!("{}.posts", self.saga);
            loop {
                let page = self.posts.page(self.user, POSTS_PAGE).await?;
                if page.is_empty() {
                    return Ok(());
                }
                let deleted = run_checkpointed(&job, &page, self.checkpoints, |_, post| {
                    self.posts.delete(self.user, post)
                })
                .await?;
                if deleted == 0 {
                    return Err(Failure::Transient(format!(
                        "у пользователя {} остались посты, которые уже отмечены удалёнными",
                        self.user
                    )));
                }
            }
        })
    }

    fn compensate(&self) -> StepFuture<'_> {
        Box::pin(async { Ok(()) })
    }
}

struct EraseProfile<'a> {
    profiles: &'a dyn ProfileKeeper,
    user: u64,
}

impl Step for EraseProfile<'_> {
    fn name(&self) -> &str {
        "profile"
    }

    fn forward_only(&self) -> bool {
        true
    }

    fn run(&self) -> StepFuture<'_> {
        self.profiles.erase(self.user)
    }

    fn compensate(&self) -> StepFuture<'_> {
        Box::pin(async { Ok(()) })
    }
}

struct EraseAccount<'a> {
    auth: &'a AuthService,
    user: u64,
}

impl Step for EraseAccount<'_> {
    fn name(&self) -> &str {
        "account"
    }

    fn forward_only(&self) -> bool {
        true
    }

    fn run(&self) -> StepFuture<'_> {
        Box::pin(async { self.auth.erase(self.user).await.map_err(storage) })
    }

    fn compensate(&self) -> StepFuture<'_> {
        Box::pin(async { Ok(()) })
    }
}

impl AuthService {
    pub async fn request_deletion(
        &self,
        access: &str,
        queue: &dyn DeletionQueue,
    ) -> Result<String, AuthError> {
        let principal = self.validate(access).await?;
        let saga = format!("delete.{}.{}", principal.user_id, self.clock.now());
        queue
            .request(saga.clone(), principal.user_id)
            .await
            .map_err(|failure| AuthError::Queue(failure.to_string()))?;
        Ok(saga)
    }

    pub async fn delete_account(
        &self,
        saga: &str,
        user: u64,
        deletion: &Deletion,
    ) -> Result<(), SagaError> {
        Saga::new(saga, &deletion.log)
            .step(Freeze { auth: self, user })
            .step(Hide {
                profiles: deletion.profiles.as_ref(),
                user,
            })
            .step(Posts {
                posts: deletion.posts.as_ref(),
                checkpoints: &deletion.checkpoints,
                saga: saga.to_string(),
                user,
            })
            .step(EraseProfile {
                profiles: deletion.profiles.as_ref(),
                user,
            })
            .step(EraseAccount { auth: self, user })
            .run()
            .await
    }

    pub async fn freeze(&self, user: u64, frozen: bool) -> Result<(), AuthError> {
        let Some(mut account) = self.accounts.get(user).await else {
            return Ok(());
        };
        if frozen {
            let closed = self.sessions.revoke_user(user, None).await;
            self.announce(user, &closed).await;
        }
        if account.frozen != frozen {
            account.frozen = frozen;
            self.accounts.update(account).await?;
        }
        Ok(())
    }

    pub async fn erase(&self, user: u64) -> Result<(), AuthError> {
        let Some(mut account) = self.accounts.get(user).await else {
            return Ok(());
        };
        account.deleted = true;
        self.accounts.update(account).await?;
        Ok(())
    }
}
