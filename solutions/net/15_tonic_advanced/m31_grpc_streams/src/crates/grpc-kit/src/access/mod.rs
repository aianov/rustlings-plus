use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use proto::REQUEST_ID;
use tokio::time::Instant;
use tower::{Layer, Service};

use crate::caller::USER_ID;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Access {
    pub service: &'static str,
    pub request_id: String,
    pub user: Option<String>,
    pub path: String,
    pub code: i32,
    pub elapsed: Duration,
}

impl fmt::Display for Access {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let user = self.user.as_deref().unwrap_or("-");
        write!(
            f,
            "{} [{}] u={} {} {} {}ms",
            self.service,
            self.request_id,
            user,
            self.path,
            self.code,
            self.elapsed.as_millis()
        )
    }
}

type Write = Arc<dyn Fn(&Access) + Send + Sync>;

#[derive(Clone)]
pub struct LogSink(Write);

impl LogSink {
    pub fn new(write: impl Fn(&Access) + Send + Sync + 'static) -> LogSink {
        LogSink(Arc::new(write))
    }

    pub fn stderr() -> LogSink {
        LogSink::new(|access| eprintln!("{access}"))
    }

    pub fn write(&self, access: &Access) {
        (self.0)(access);
    }
}

impl fmt::Debug for LogSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LogSink")
    }
}

impl PartialEq for LogSink {
    fn eq(&self, other: &LogSink) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for LogSink {}

#[derive(Debug, Clone)]
pub struct AccessLog {
    service: &'static str,
    sink: Option<LogSink>,
}

impl AccessLog {
    pub fn new(service: &'static str, sink: Option<LogSink>) -> AccessLog {
        AccessLog { service, sink }
    }
}

impl<S> Layer<S> for AccessLog {
    type Service = AccessService<S>;

    fn layer(&self, inner: S) -> AccessService<S> {
        AccessService {
            inner,
            service: self.service,
            sink: self.sink.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AccessService<S> {
    inner: S,
    service: &'static str,
    sink: Option<LogSink>,
}

fn header(headers: &http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

type Pending<R, E> = Pin<Box<dyn Future<Output = Result<R, E>> + Send>>;

impl<S, B, RB> Service<http::Request<B>> for AccessService<S>
where
    S: Service<http::Request<B>, Response = http::Response<RB>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pending<S::Response, S::Error>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: http::Request<B>) -> Self::Future {
        let ready = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, ready);
        let Some(sink) = self.sink.clone() else {
            return Box::pin(inner.call(request));
        };
        let service = self.service;
        let path = request.uri().path().to_string();
        let request_id = header(request.headers(), REQUEST_ID).unwrap_or_else(|| "-".to_string());
        let user = header(request.headers(), USER_ID);
        let started = Instant::now();
        Box::pin(async move {
            let response = inner.call(request).await?;
            let code = header(response.headers(), "grpc-status")
                .and_then(|code| code.parse().ok())
                .unwrap_or(0);
            sink.write(&Access {
                service,
                request_id,
                user,
                path,
                code,
                elapsed: started.elapsed(),
            });
            Ok(response)
        })
    }
}
