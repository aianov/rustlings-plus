use std::future::Future;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout};

use crate::method::Method;
use crate::parse::{Limits, Parsed, parse_request};
use crate::request::Request;
use crate::response::Response;
use crate::router::Router;

pub struct Server {
    router: Arc<Router>,
    limits: Limits,
    read_timeout: Duration,
}

impl Server {
    pub fn new(router: Router) -> Server {
        Server {
            router: Arc::new(router),
            limits: Limits::default(),
            read_timeout: Duration::from_secs(10),
        }
    }

    pub fn limits(mut self, limits: Limits) -> Server {
        self.limits = limits;
        self
    }

    pub fn read_timeout(mut self, read_timeout: Duration) -> Server {
        self.read_timeout = read_timeout;
        self
    }

    pub async fn serve(
        self,
        listener: TcpListener,
        shutdown: impl Future<Output = ()>,
    ) -> io::Result<()> {
        let mut connections = JoinSet::new();
        tokio::pin!(shutdown);
        loop {
            let accepted = tokio::select! {
                biased;
                () = &mut shutdown => break,
                Some(_) = connections.join_next(), if !connections.is_empty() => continue,
                accepted = listener.accept() => accepted,
            };
            match accepted {
                Ok((stream, _)) => {
                    let _ = stream.set_nodelay(true);
                    let router = Arc::clone(&self.router);
                    connections.spawn(connection(stream, router, self.limits, self.read_timeout));
                }
                Err(..) => sleep(Duration::from_millis(100)).await,
            }
        }
        drop(listener);
        while connections.join_next().await.is_some() {}
        Ok(())
    }
}

async fn read_request(
    stream: &mut TcpStream,
    limits: &Limits,
    read_timeout: Duration,
) -> Result<Option<Request>, Response> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match parse_request(&buf, limits) {
            Ok(Parsed::Complete { request, .. }) => return Ok(Some(request)),
            Ok(Parsed::Partial) => {}
            Err(error) => return Err(Response::error(error.status(), &error.to_string())),
        }
        match timeout(read_timeout, stream.read(&mut chunk)).await {
            Ok(Ok(0)) | Ok(Err(..)) => return Ok(None),
            Ok(Ok(n)) => buf.extend_from_slice(&chunk[..n]),
            Err(..) => return Err(Response::error(408, "запрос не пришёл вовремя")),
        }
    }
}

async fn run_handler(router: Arc<Router>, request: Request) -> Response {
    match tokio::spawn(async move { router.dispatch(request).await }).await {
        Ok(response) => response,
        Err(..) => Response::error(500, "обработчик упал"),
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

async fn connection(
    mut stream: TcpStream,
    router: Arc<Router>,
    limits: Limits,
    read_timeout: Duration,
) {
    let (head_only, mut response) = match read_request(&mut stream, &limits, read_timeout).await {
        Ok(Some(request)) => (
            request.method == Method::Head,
            run_handler(router, request).await,
        ),
        Ok(None) => return,
        Err(response) => (false, response),
    };
    response.headers.set("Connection", "close");
    let _ = stream.write_all(&response.to_bytes(head_only)).await;
    finish(stream).await;
}
