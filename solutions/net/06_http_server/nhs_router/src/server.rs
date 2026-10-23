use std::io;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

use crate::{Limits, Method, Parsed, Request, Response, Router, parse_request};

pub async fn serve(listener: TcpListener, router: Router) -> io::Result<()> {
    let router = Arc::new(router);
    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(connection(stream, router.clone()));
    }
}

async fn run_handler(router: Arc<Router>, request: Request) -> Response {
    match tokio::spawn(async move { router.dispatch(request).await }).await {
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

async fn connection(mut stream: TcpStream, router: Arc<Router>) {
    let limits = Limits::default();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let request = loop {
        match parse_request(&buf, &limits) {
            Ok(Parsed::Complete { request, .. }) => break request,
            Ok(Parsed::Partial) => {}
            Err(e) => {
                let mut response = Response::error(e.status(), &format!("{e:?}"));
                response.headers.append("Connection", "close");
                let _ = stream.write_all(&response.to_bytes(false)).await;
                return finish(stream).await;
            }
        }
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };
    let head_only = request.method == Method::Head;
    let mut response = run_handler(router, request).await;
    response.headers.append("Connection", "close");
    let _ = stream.write_all(&response.to_bytes(head_only)).await;
    finish(stream).await;
}
