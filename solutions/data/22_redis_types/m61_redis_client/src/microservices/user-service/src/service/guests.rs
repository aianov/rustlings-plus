use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use contracts::UserId;

use super::{UserError, UserService};
use crate::common::User;

pub const GUESTS_KEPT: usize = 10;

pub type BoxGuests<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait Guests: Send + Sync {
    fn visited(&self, owner: UserId, guest: UserId) -> BoxGuests<'_, ()>;
    fn recent(&self, owner: UserId, limit: usize) -> BoxGuests<'_, Vec<UserId>>;
}

impl UserService {
    pub fn with_guests(self, guests: Arc<dyn Guests>) -> UserService {
        UserService {
            guests: Some(guests),
            ..self
        }
    }

    pub async fn visited(&self, owner: UserId, guest: Option<UserId>) {
        if let (Some(guests), Some(guest)) = (&self.guests, guest)
            && guest != owner
        {
            guests.visited(owner, guest).await;
        }
    }

    pub async fn guests_of(&self, owner: UserId) -> Result<Vec<User>, UserError> {
        let Some(guests) = &self.guests else {
            return Err(UserError::NoPostgres);
        };
        let mut found = Vec::new();
        for id in guests.recent(owner, GUESTS_KEPT).await {
            if let Ok(user) = self.profile(id).await {
                found.push(user);
            }
        }
        Ok(found)
    }
}
