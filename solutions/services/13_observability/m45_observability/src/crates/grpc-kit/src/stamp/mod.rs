use std::time::Duration;

use proto::{REQUEST_ID, new_request_id, request_id};
use service_runtime::{TRACEPARENT, TraceContext, current_trace};
use tokio::time::Instant;
use tonic::service::Interceptor;
use tonic::{Request, Status};

use crate::budget::budget_of;
use crate::caller::{Caller, CallerRole, USER_ID, USER_ROLE};

pub const TRACE_ID: &str = "x-trace-id";

pub const LANGUAGE: &str = "accept-language";

pub const CLIENT_KEYS: [&str; 2] = [TRACE_ID, LANGUAGE];

pub const CLIENT_VALUE_MAX: usize = 128;

pub fn client_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= CLIENT_VALUE_MAX
        && value.bytes().all(|byte| (b' '..=b'~').contains(&byte))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamp {
    request_id: String,
    user: Option<u64>,
    role: Option<String>,
    deadline: Option<Instant>,
    passed: Vec<(&'static str, String)>,
    trace: Option<TraceContext>,
}

impl Stamp {
    pub fn new(request_id: &str) -> Stamp {
        Stamp {
            request_id: request_id.to_string(),
            user: None,
            role: None,
            deadline: None,
            passed: Vec::new(),
            trace: None,
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
        let role = request
            .extensions()
            .get::<CallerRole>()
            .map(|role| role.0.clone());
        let stamp = Stamp {
            request_id: request_id(request.metadata()),
            user,
            role,
            deadline: budget_of(request).map(|budget| Instant::now() + budget),
            passed: Vec::new(),
            trace: request
                .metadata()
                .get(TRACEPARENT)
                .and_then(|value| value.to_str().ok())
                .and_then(TraceContext::parse)
                .or_else(current_trace),
        };
        CLIENT_KEYS.into_iter().fold(stamp, |stamp, key| {
            match request.metadata().get(key).and_then(|v| v.to_str().ok()) {
                Some(value) => stamp.pass(key, value),
                None => stamp,
            }
        })
    }

    pub fn pass(mut self, key: &str, value: &str) -> Stamp {
        let key = key.to_ascii_lowercase();
        if key == TRACEPARENT {
            return match TraceContext::parse(value) {
                Some(parent) => self.in_trace(parent.child()),
                None => self,
            };
        }
        let Some(known) = CLIENT_KEYS.into_iter().find(|own| *own == key) else {
            return self;
        };
        if client_value(value) {
            self.passed.retain(|(own, _)| *own != known);
            self.passed.push((known, value.to_string()));
        }
        self
    }

    pub fn passed(&self) -> &[(&'static str, String)] {
        &self.passed
    }

    pub fn trace_id(&self) -> Option<&str> {
        self.passed
            .iter()
            .find(|(key, _)| *key == TRACE_ID)
            .map(|(_, value)| value.as_str())
    }

    pub fn in_trace(self, trace: TraceContext) -> Stamp {
        Stamp {
            trace: Some(trace),
            ..self
        }
    }

    pub fn trace(&self) -> Option<&TraceContext> {
        self.trace.as_ref()
    }

    pub fn user(self, id: u64) -> Stamp {
        Stamp {
            user: Some(id),
            ..self
        }
    }

    pub fn role(self, role: &str) -> Stamp {
        Stamp {
            role: Some(role.to_string()),
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

    pub fn role_name(&self) -> Option<&str> {
        self.role.as_deref()
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
        if let Some(role) = &self.role {
            if let Ok(value) = role.parse() {
                request.metadata_mut().insert(USER_ROLE, value);
            }
        }
        for (key, value) in &self.passed {
            if let Ok(value) = value.parse() {
                request.metadata_mut().insert(*key, value);
            }
        }
        if let Some(trace) = self.trace.clone().or_else(current_trace) {
            if let Ok(value) = trace.header().parse() {
                request.metadata_mut().insert(TRACEPARENT, value);
            }
        }
        Ok(request)
    }
}
