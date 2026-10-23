mod write;

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};
use tokio::task::JoinSet;
use tokio::time::{Instant, sleep, sleep_until, timeout};

use crate::method::Method;
use crate::parse::{Limits, Parsed, parse_head, parse_request};
use crate::proxy::announced;
use crate::request::{Request, Version};
use crate::response::Response;
use crate::router::{Handler, Router};
use crate::upgrade::Upgraded;
use write::{finish, write_response};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub limits: Limits,
    pub idle_timeout: Duration,
    pub head_timeout: Duration,
    pub body_timeout: Duration,
    pub max_connections: usize,
    pub drain_timeout: Duration,
    pub proxy_protocol: bool,
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
            proxy_protocol: false,
        }
    }
}

pub struct Server {
    app: Handler,
    config: ServerConfig,
}

impl Server {
    pub fn new(router: Router) -> Server {
        Server::with_handler(router.into_handler())
    }

    pub fn with_handler(app: Handler) -> Server {
        Server {
            app,
            config: ServerConfig::default(),
        }
    }

    pub fn config(mut self, config: ServerConfig) -> Server {
        self.config = config;
        self
    }

    pub fn limits(mut self, limits: Limits) -> Server {
        self.config.limits = limits;
        self
    }

    pub async fn serve(
        self,
        listener: TcpListener,
        shutdown: impl Future<Output = ()>,
    ) -> io::Result<()> {
        let config = Arc::new(self.config);
        let permits = Arc::new(Semaphore::new(config.max_connections.max(1)));
        let (stop, stopping) = watch::channel(false);
        let mut connections = JoinSet::new();
        tokio::pin!(shutdown);
        loop {
            let permit = tokio::select! {
                biased;
                () = &mut shutdown => break,
                permit = Arc::clone(&permits).acquire_owned() => match permit {
                    Ok(permit) => permit,
                    Err(..) => break,
                },
            };
            tokio::select! {
                biased;
                () = &mut shutdown => break,
                accepted = listener.accept() => match accepted {
                    Ok((stream, peer)) => {
                        let _ = stream.set_nodelay(true);
                        let app = Arc::clone(&self.app);
                        let work = connection(stream, peer, app, Arc::clone(&config), stopping.clone(), permit);
                        connections.spawn(work);
                    }
                    Err(..) => sleep(Duration::from_millis(10)).await,
                },
            }
            while connections.try_join_next().is_some() {}
        }
        drop(listener);
        let _ = stop.send(true);
        let drain = async { while connections.join_next().await.is_some() {} };
        if timeout(config.drain_timeout, drain).await.is_err() {
            connections.abort_all();
            while connections.join_next().await.is_some() {}
        }
        Ok(())
    }
}

async fn run_handler(app: Handler, request: Request) -> Response {
    match tokio::spawn(async move { app(request).await }).await {
        Ok(response) => response,
        Err(..) => Response::error(500, "обработчик упал"),
    }
}

enum Next {
    Request(Request),
    Close,
    Reject(u16, String),
}

async fn next_request(
    stream: &mut TcpStream,
    buf: &mut Vec<u8>,
    config: &ServerConfig,
    stopping: &mut watch::Receiver<bool>,
) -> Next {
    let mut started = (!buf.is_empty()).then(Instant::now);
    let mut head_done = None;
    let mut sent_continue = false;
    let mut chunk = [0u8; 8192];
    loop {
        if !buf.is_empty() {
            match parse_head(buf, &config.limits) {
                Err(error) => return Next::Reject(error.status(), error.to_string()),
                Ok(None) => {}
                Ok(Some(head)) => {
                    if head.has_unknown_expectation() {
                        return Next::Reject(417, String::from("такого Expect сервер не знает"));
                    }
                    match parse_request(buf, &config.limits) {
                        Err(error) => return Next::Reject(error.status(), error.to_string()),
                        Ok(Parsed::Complete { request, consumed }) => {
                            buf.drain(..consumed);
                            return Next::Request(request);
                        }
                        Ok(Parsed::Partial) => {
                            head_done.get_or_insert_with(Instant::now);
                            if head.expects_continue() && !sent_continue {
                                sent_continue = true;
                                if stream
                                    .write_all(b"HTTP/1.1 100 Continue\r\n\r\n")
                                    .await
                                    .is_err()
                                {
                                    return Next::Close;
                                }
                            }
                        }
                    }
                }
            }
        }
        let idle = buf.is_empty();
        if idle && *stopping.borrow() {
            return Next::Close;
        }
        let deadline = match (head_done, started) {
            (Some(done), _) => done + config.body_timeout,
            (None, Some(start)) => start + config.head_timeout,
            (None, None) => Instant::now() + config.idle_timeout,
        };
        tokio::select! {
            read = stream.read(&mut chunk) => match read {
                Ok(0) | Err(..) => return Next::Close,
                Ok(n) => {
                    started.get_or_insert_with(Instant::now);
                    buf.extend_from_slice(&chunk[..n]);
                }
            },
            () = sleep_until(deadline) => {
                if idle {
                    return Next::Close;
                }
                return Next::Reject(408, String::from("запрос не пришёл вовремя"));
            }
            _ = stopping.changed(), if idle => return Next::Close,
        }
    }
}

fn mark(response: &mut Response, version: Version, keep: bool) {
    if !keep {
        response.headers.set("Connection", "close");
    } else if version == Version::Http10 {
        response.headers.set("Connection", "keep-alive");
    }
}

async fn connection(
    mut stream: TcpStream,
    peer: SocketAddr,
    app: Handler,
    config: Arc<ServerConfig>,
    mut stopping: watch::Receiver<bool>,
    _permit: OwnedSemaphorePermit,
) {
    let mut buf = Vec::new();
    let peer = if config.proxy_protocol {
        match announced(&mut stream, &mut buf, config.head_timeout).await {
            Some(peer) => peer,
            None => return,
        }
    } else {
        Some(peer)
    };
    loop {
        let request = match next_request(&mut stream, &mut buf, &config, &mut stopping).await {
            Next::Request(mut request) => {
                request.peer = peer;
                request
            }
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
        let mut response = run_handler(Arc::clone(&app), request).await;
        if response.status.code() == 101 {
            if let Some(hook) = response.upgrade.take() {
                let head = response.to_bytes(true);
                if stream.write_all(&head).await.is_err() || stream.flush().await.is_err() {
                    return;
                }
                let rest = std::mem::take(&mut buf);
                (hook.0)(Upgraded { stream, rest }).await;
                return;
            }
        }
        let raw_stream = response.stream.is_some() && version == Version::Http10;
        let keep = wants_keep && !*stopping.borrow() && !raw_stream;
        mark(&mut response, version, keep);
        if write_response(&mut stream, response, version, head_only)
            .await
            .is_err()
        {
            return;
        }
        if !keep {
            return finish(stream).await;
        }
    }
}
