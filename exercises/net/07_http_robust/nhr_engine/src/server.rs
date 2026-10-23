use std::future::Future;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};
use tokio::task::JoinSet;
use tokio::time::{Instant, sleep_until, timeout};

use crate::{Handler, Limits, Method, Parsed, Request, Response, Router, Version, parse_head, parse_request};

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub limits: Limits,
    pub idle_timeout: Duration,
    pub head_timeout: Duration,
    pub body_timeout: Duration,
    pub max_connections: usize,
    pub drain_timeout: Duration,
}

impl Default for ServerConfig {
    fn default() -> ServerConfig {
        ServerConfig {
            limits: Limits::default(),
            idle_timeout: Duration::from_secs(60),
            head_timeout: Duration::from_secs(10),
            body_timeout: Duration::from_secs(30),
            max_connections: 1024,
            drain_timeout: Duration::from_secs(30),
        }
    }
}

pub async fn serve(listener: TcpListener, router: Router) -> io::Result<()> {
    serve_with(listener, router.into_handler(), ServerConfig::default(), std::future::pending()).await
}

pub async fn serve_with<S>(listener: TcpListener, app: Handler, config: ServerConfig, shutdown: S) -> io::Result<()>
where
    S: Future<Output = ()> + Send,
{
    todo!()
}

async fn run_handler(app: Handler, request: Request) -> Response {
    match tokio::spawn(async move { app(request).await }).await {
        Ok(response) => response,
        Err(_) => Response::error(500, "internal error"),
    }
}

async fn finish(mut stream: TcpStream) {
    let _ = stream.shutdown().await;
    let drain = async {
        let mut sink = [0u8; 4096];
        let mut total = 0;
        while let Ok(n) = stream.read(&mut sink).await {
            total += n;
            if n == 0 || total > 1 << 20 {
                break;
            }
        }
    };
    let _ = timeout(Duration::from_secs(1), drain).await;
}

fn mark_connection(response: &mut Response, version: Version, keep: bool) {
    if !keep {
        response.headers.set("Connection", "close");
    } else if version == Version::Http10 {
        response.headers.set("Connection", "keep-alive");
    }
}

enum Next {
    Request(Request),
    Close,
    Reject(u16, String),
}

async fn next_request(stream: &mut TcpStream, buf: &mut Vec<u8>, config: &ServerConfig, stop: &mut watch::Receiver<bool>) -> Next {
    todo!()
}

async fn write_response(stream: &mut TcpStream, mut response: Response, version: Version, head_only: bool) -> io::Result<()> {
    todo!()
}

async fn connection(
    mut stream: TcpStream,
    app: Handler,
    config: Arc<ServerConfig>,
    mut stop: watch::Receiver<bool>,
    _permit: OwnedSemaphorePermit,
) {
    todo!()
}
