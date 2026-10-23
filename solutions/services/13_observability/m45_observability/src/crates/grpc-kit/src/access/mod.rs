use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use proto::REQUEST_ID;
use service_runtime::{TRACEPARENT, TraceContext, metrics, with_trace};
use tokio::time::Instant;
use tower::{Layer, Service};
use tracing::Instrument;

use crate::caller::USER_ID;

pub const ACCESS_LOG_ENV: &str = "ACCESS_LOG";

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
        LogSink::new(|access| {
            tracing::info!(
                target: "access",
                request_id = %access.request_id,
                method = %access.path,
                code = access.code,
                ms = u64::try_from(access.elapsed.as_millis()).unwrap_or(u64::MAX),
                "{access}"
            );
        })
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
        let service = self.service;
        let sink = self.sink.clone();
        let path = request.uri().path().to_string();
        let request_id = header(request.headers(), REQUEST_ID).unwrap_or_else(|| "-".to_string());
        let user = header(request.headers(), USER_ID);
        let trace = TraceContext::continue_from(header(request.headers(), TRACEPARENT).as_deref());
        let span = tracing::info_span!(
            "grpc",
            server = service,
            method = %path,
            request_id = %request_id,
            trace_id = %trace.trace_id,
            user = user.as_deref().unwrap_or("-"),
        );
        let started = Instant::now();
        let work = with_trace(trace, inner.call(request));
        Box::pin(
            async move {
                let response = work.await?;
                let code = header(response.headers(), "grpc-status")
                    .and_then(|code| code.parse().ok())
                    .unwrap_or(0);
                let elapsed = started.elapsed();
                record(service, &path, code, elapsed);
                if let Some(sink) = sink {
                    sink.write(&Access {
                        service,
                        request_id,
                        user,
                        path,
                        code,
                        elapsed,
                    });
                }
                Ok(response)
            }
            .instrument(span),
        )
    }
}

pub const GRPC_REQUESTS: &str = "grpc_server_requests_total";

pub const GRPC_DURATION: &str = "grpc_server_request_duration_seconds";

fn record(service: &str, method: &str, code: i32, elapsed: Duration) {
    let code = code.to_string();
    metrics().inc(
        GRPC_REQUESTS,
        "gRPC-вызовы по методу и коду ответа",
        &[("service", service), ("method", method), ("code", &code)],
        1,
    );
    metrics().observe(
        GRPC_DURATION,
        "время gRPC-вызова в секундах",
        &[("service", service), ("method", method)],
        elapsed.as_secs_f64(),
    );
}
