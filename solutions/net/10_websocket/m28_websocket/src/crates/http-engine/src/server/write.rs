use std::io;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::request::Version;
use crate::response::Response;

pub(super) async fn finish(mut stream: TcpStream) {
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

pub(super) async fn write_response(
    stream: &mut TcpStream,
    mut response: Response,
    version: Version,
    head_only: bool,
) -> io::Result<()> {
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
