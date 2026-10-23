// Журнал вызовов: путь, идентификатор запроса, код ответа и время. Слой tower видит запрос
// и ответ целиком, в отличие от перехватчика. Допиши call.
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tower::{Layer, Service};

pub mod pb {
    tonic::include_proto!("users.v1");
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogLine {
    pub request_id: String,
    pub path: String,
    pub status: i32,
    pub elapsed: Duration,
}

pub type Sink = Arc<Mutex<Vec<LogLine>>>;

#[derive(Clone)]
pub struct LogLayer {
    sink: Sink,
}

impl LogLayer {
    pub fn new(sink: Sink) -> Self {
        LogLayer { sink }
    }
}

impl<S> Layer<S> for LogLayer {
    type Service = LogService<S>;

    fn layer(&self, inner: S) -> LogService<S> {
        LogService { inner, sink: self.sink.clone() }
    }
}

#[derive(Clone)]
pub struct LogService<S> {
    inner: S,
    sink: Sink,
}

impl<S, B, RB> Service<http::Request<B>> for LogService<S>
where
    S: Service<http::Request<B>, Response = http::Response<RB>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: http::Request<B>) -> Self::Future {
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let sink = self.sink.clone();
        let path = request.uri().path().to_string();
        let request_id = request.headers().get("x-request-id").and_then(|v| v.to_str().ok()).unwrap_or("-").to_string();
        let started = Instant::now();
        Box::pin(async move {
            let response = inner.call(request).await?;
            let status = response.headers().get("grpc-status").and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok()).unwrap_or(0);
            sink.lock().unwrap().push(LogLine { request_id, path, status, elapsed: started.elapsed() });
            Ok(response)
        })
    }
}
