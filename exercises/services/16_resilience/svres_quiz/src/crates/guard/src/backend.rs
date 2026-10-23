use crate::deadline::Deadline;
use crate::retry::Status;
use std::future::Future;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub body: String,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response(pub String);

/// Всё, что стоит за шлюзом: сервисы по сети. Срок запроса едет вместе с ним.
pub trait Backend: Send + Sync + 'static {
    fn call(&self, dep: &'static str, req: Request, deadline: Deadline) -> impl Future<Output = Result<Response, Status>> + Send;
}

impl<T: Backend> Backend for std::sync::Arc<T> {
    fn call(&self, dep: &'static str, req: Request, deadline: Deadline) -> impl Future<Output = Result<Response, Status>> + Send {
        (**self).call(dep, req, deadline)
    }
}
