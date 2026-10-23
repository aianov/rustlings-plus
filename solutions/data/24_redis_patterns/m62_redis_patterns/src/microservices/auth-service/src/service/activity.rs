use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, PoisonError};
use std::time::Duration;

use super::token;
use super::{AuthError, AuthService, Issued, ThrottleKey};

pub const SESSION_IDLE: Duration = Duration::from_secs(30 * 60);

pub type BoxActivity<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait Activity: Send + Sync {
    fn strikes(&self, keys: Vec<ThrottleKey>) -> BoxActivity<'_, u32>;
    fn failed(&self, keys: Vec<ThrottleKey>) -> BoxActivity<'_, ()>;
    fn cleared(&self, account: String) -> BoxActivity<'_, ()>;
    fn opened(&self, user: u64, session: String) -> BoxActivity<'_, ()>;
    fn touched(&self, session: String) -> BoxActivity<'_, bool>;
    fn closed_all(&self, user: u64) -> BoxActivity<'_, usize>;
}

impl AuthService {
    pub fn with_activity(self, activity: Arc<dyn Activity>) -> AuthService {
        AuthService {
            activity: Some(activity),
            ..self
        }
    }

    pub(super) fn activity(&self) -> Option<&Arc<dyn Activity>> {
        self.activity.as_ref()
    }

    pub(super) async fn session_opened(&self, issued: &Issued) {
        let Some(activity) = &self.activity else {
            return;
        };
        let now = self.clock.now();
        let verified = {
            let keys = self.keys.read().unwrap_or_else(PoisonError::into_inner);
            token::verify(&keys, &issued.access, &self.policy, now)
        };
        if let Ok(claims) = verified {
            activity.opened(claims.sub, claims.sid).await;
        }
    }

    pub(super) async fn still_active(&self, session: &str) -> Result<(), AuthError> {
        let Some(activity) = &self.activity else {
            return Ok(());
        };
        match activity.touched(session.to_string()).await {
            true => Ok(()),
            false => Err(AuthError::TokenExpired),
        }
    }

    pub(super) async fn forget_sessions(&self, user: u64) -> usize {
        match &self.activity {
            Some(activity) => activity.closed_all(user).await,
            None => 0,
        }
    }
}
