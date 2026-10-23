mod client;
mod http;
mod input;
mod render;
mod ws;

use std::io;
use std::time::Duration;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite};
use tokio::time::sleep;

pub use http::{Body, Reply, Web};
pub use input::{Command, HELP, parse};
pub use ws::{Ws, accept_for};

pub const HTTP_ADDR_ENV: &str = "GATEWAY_HTTP_ADDR";

pub const DEFAULT_HTTP_ADDR: &str = "127.0.0.1:7880";

pub const AUTH_ADDR_ENV: &str = "AUTH_HTTP_ADDR";

pub const DEFAULT_AUTH_ADDR: &str = "127.0.0.1:7881";

pub async fn connect(addr: &str, token: &str, attempts: u32) -> io::Result<Ws> {
    let path = format!("/ws?token={token}");
    let mut last = io::Error::other("шлюз не ответил");
    for _ in 0..attempts.max(1) {
        match Ws::connect(addr, &path).await {
            Ok(stream) => return Ok(stream),
            Err(error) => last = error,
        }
        sleep(Duration::from_millis(100)).await;
    }
    Err(last)
}

pub async fn run<R, W>(gateway: &str, auth: &str, input: R, output: W) -> io::Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut client = client::Client::new(gateway, auth, output);
    client.wait_ready(50).await?;
    let mut lines = input.lines();
    loop {
        tokio::select! {
            line = lines.next_line() => match line? {
                Some(line) => {
                    if !client.command(&line).await? {
                        client.hang_up().await?;
                        break;
                    }
                }
                None => {
                    client.hang_up().await?;
                    break;
                }
            },
            incoming = client.incoming() => match incoming {
                Ok(Some(raw)) => client.unsolicited(&raw).await?,
                Ok(None) | Err(..) => client.lost().await?,
            },
        }
    }
    Ok(())
}
