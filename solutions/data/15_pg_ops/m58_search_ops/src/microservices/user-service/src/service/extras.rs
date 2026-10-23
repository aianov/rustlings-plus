use std::sync::Arc;

use contracts::UserId;

use super::{UserError, UserService};
use crate::common::{Settings, User};
use crate::repository::ProfileExtras;

pub const MAX_PEOPLE: usize = 20;

pub const MAX_PEOPLE_QUERY: usize = 32;

impl UserService {
    pub fn with_extras(self, extras: Arc<dyn ProfileExtras>) -> UserService {
        UserService {
            extras: Some(extras),
            ..self
        }
    }

    fn extras(&self) -> Result<&dyn ProfileExtras, UserError> {
        self.extras.as_deref().ok_or(UserError::NoPostgres)
    }

    pub async fn people(&self, query: &str, limit: usize) -> Result<Vec<User>, UserError> {
        let query = query.trim();
        if query.is_empty() || query.chars().count() > MAX_PEOPLE_QUERY {
            return Err(UserError::BadSettings(format!(
                "ник для поиска это 1..={MAX_PEOPLE_QUERY} символов"
            )));
        }
        let limit = match limit {
            0 => 10,
            limit => limit.min(MAX_PEOPLE),
        };
        Ok(self.extras()?.people(query.to_string(), limit).await)
    }

    pub async fn settings(&self, user: UserId) -> Result<Settings, UserError> {
        let found = self.extras()?.settings(user).await;
        found.ok_or(UserError::UnknownUser(user))
    }

    pub async fn patch_settings(
        &self,
        user: UserId,
        patch: Settings,
    ) -> Result<Settings, UserError> {
        if let Some(problem) = patch.problem() {
            return Err(UserError::BadSettings(problem));
        }
        let saved = self.extras()?.patch_settings(user, patch).await;
        saved.ok_or(UserError::UnknownUser(user))
    }
}
