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
    let config = Arc::new(config);
    let permits = Arc::new(Semaphore::new(config.max_connections));
    let (stop_tx, stop_rx) = watch::channel(false);
    let mut connections = JoinSet::new();
    tokio::pin!(shutdown);
    loop {
        let permit = tokio::select! {
            _ = &mut shutdown => break,
            permit = permits.clone().acquire_owned() => permit.expect("семафор не закрывается"),
        };
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    connections.spawn(connection(stream, app.clone(), config.clone(), stop_rx.clone(), permit));
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            },
        }
        while connections.try_join_next().is_some() {}
    }
    drop(listener);
    let _ = stop_tx.send(true);
    let drained = timeout(config.drain_timeout, async { while connections.join_next().await.is_some() {} }).await;
    if drained.is_err() {
        connections.abort_all();
    }
    Ok(())
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
    let mut started = if buf.is_empty() { None } else { Some(Instant::now()) };
    let mut head_done = None;
    let mut sent_continue = false;
    let mut chunk = [0u8; 8192];
    loop {
        if !buf.is_empty() {
            match parse_head(buf, &config.limits) {
                Err(e) => return Next::Reject(e.status(), format!("{e:?}")),
                Ok(None) => {}
                Ok(Some(head)) => {
                    if head.has_unknown_expectation() {
                        return Next::Reject(417, "unsupported expectation".to_string());
                    }
                    match parse_request(buf, &config.limits) {
                        Err(e) => return Next::Reject(e.status(), format!("{e:?}")),
                        Ok(Parsed::Complete { request, consumed }) => {
                            buf.drain(..consumed);
                            return Next::Request(request);
                        }
                        Ok(Parsed::Partial) => {
                            head_done.get_or_insert_with(Instant::now);
                            if head.expects_continue() && !sent_continue {
                                sent_continue = true;
                                if stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").await.is_err() {
                                    return Next::Close;
                                }
                            }
                        }
                    }
                }
            }
        }
        let idle = buf.is_empty();
        if idle && *stop.borrow() {
            return Next::Close;
        }
        let deadline = match (head_done, started) {
            (Some(done), _) => done + config.body_timeout,
            (None, Some(start)) => start + config.head_timeout,
            (None, None) => Instant::now() + config.idle_timeout,
        };
        tokio::select! {
            read = stream.read(&mut chunk) => match read {
                Ok(0) | Err(_) => return Next::Close,
                Ok(n) => {
                    started.get_or_insert_with(Instant::now);
                    buf.extend_from_slice(&chunk[..n]);
                }
            },
            _ = sleep_until(deadline) => {
                return if idle { Next::Close } else { Next::Reject(408, "request timeout".to_string()) };
            }
            _ = stop.changed(), if idle => return Next::Close,
        }
    }
}

async fn write_response(stream: &mut TcpStream, mut response: Response, version: Version, head_only: bool) -> io::Result<()> {
    if response.stream.is_none() {
        return stream.write_all(&response.to_bytes(head_only)).await;
    }
    let chunked = version == Version::Http11;
    if chunked {
        response.headers.set("Transfer-Encoding", "chunked");
    }
    stream.write_all(&response.to_bytes(true)).await?;
    let Some(mut chunks) = response.stream.take().filter(|_| !head_only) else {
        return Ok(());
    };
    while let Some(data) = chunks.recv().await {
        if data.is_empty() {
            continue;
        }
        if chunked {
            let mut frame = format!("{:x}\r\n", data.len()).into_bytes();
            frame.extend_from_slice(&data);
            frame.extend_from_slice(b"\r\n");
            stream.write_all(&frame).await?;
        } else {
            stream.write_all(&data).await?;
        }
    }
    if chunked {
        stream.write_all(b"0\r\n\r\n").await?;
    }
    Ok(())
}

async fn connection(
    mut stream: TcpStream,
    app: Handler,
    config: Arc<ServerConfig>,
    mut stop: watch::Receiver<bool>,
    _permit: OwnedSemaphorePermit,
) {
    let mut buf = Vec::new();
    loop {
        let request = match next_request(&mut stream, &mut buf, &config, &mut stop).await {
            Next::Request(request) => request,
            Next::Close => return,
            Next::Reject(status, message) => {
                let mut response = Response::error(status, &message);
                response.headers.set("Connection", "close");
                let _ = stream.write_all(&response.to_bytes(false)).await;
                return finish(stream).await;
            }
        };
        let version = request.version;
        let head_only = request.method == Method::Head;
        let wants_keep = request.keep_alive();
        let mut response = run_handler(app.clone(), request).await;
        let keep = wants_keep && !*stop.borrow() && !(response.stream.is_some() && version == Version::Http10);
        mark_connection(&mut response, version, keep);
        if write_response(&mut stream, response, version, head_only).await.is_err() {
            return;
        }
        if !keep {
            return finish(stream).await;
        }
    }
}
