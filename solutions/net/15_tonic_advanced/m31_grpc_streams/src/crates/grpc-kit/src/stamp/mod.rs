use std::time::Duration;

use proto::{REQUEST_ID, new_request_id, request_id};
use tokio::time::Instant;
use tonic::service::Interceptor;
use tonic::{Request, Status};

use crate::budget::budget_of;
use crate::caller::{Caller, USER_ID};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamp {
    request_id: String,
    user: Option<u64>,
    deadline: Option<Instant>,
}

impl Stamp {
    pub fn new(request_id: &str) -> Stamp {
        Stamp {
            request_id: request_id.to_string(),
            user: None,
            deadline: None,
        }
    }

    pub fn fresh() -> Stamp {
        Stamp::new(&new_request_id())
    }

    pub fn follow<T>(request: &Request<T>) -> Stamp {
        let user = request
            .extensions()
            .get::<Caller>()
            .map(|caller| caller.0)
            .or_else(|| {
                let raw = request.metadata().get(USER_ID)?.to_str().ok()?;
                raw.parse().ok()
            });
        Stamp {
            request_id: request_id(request.metadata()),
            user,
            deadline: budget_of(request).map(|budget| Instant::now() + budget),
        }
    }

    pub fn user(self, id: u64) -> Stamp {
        Stamp {
            user: Some(id),
            ..self
        }
    }

    pub fn until(self, deadline: Instant) -> Stamp {
        Stamp {
            deadline: Some(deadline),
            ..self
        }
    }

    pub fn within(self, budget: Duration) -> Stamp {
        self.until(Instant::now() + budget)
    }

    pub fn endless(self) -> Stamp {
        Stamp {
            deadline: None,
            ..self
        }
    }

    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn user_id(&self) -> Option<u64> {
        self.user
    }

    pub fn left(&self) -> Option<Duration> {
        self.deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
    }
}

impl Interceptor for Stamp {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        if let Some(left) = self.left() {
            if left.is_zero() {
                return Err(Status::deadline_exceeded(
                    "бюджет запроса кончился до вызова",
                ));
            }
            request.set_timeout(left);
        }
        if let Ok(value) = self.request_id.parse() {
            request.metadata_mut().insert(REQUEST_ID, value);
        }
        if let Some(user) = self.user {
            if let Ok(value) = user.to_string().parse() {
                request.metadata_mut().insert(USER_ID, value);
            }
        }
        Ok(request)
    }
}
