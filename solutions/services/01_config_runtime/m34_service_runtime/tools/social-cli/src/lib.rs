mod client;
mod http;
mod input;
mod net;
mod ready;
mod render;
mod ws;

use std::io;
use std::time::Duration;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite};
use tokio::time::sleep;

pub use http::{Body, Reply, Web};
pub use input::{Command, HELP, parse};
pub use net::{Dialer, Stream};
pub use ready::{READY_LIMIT, READY_PAUSE, await_ready};
pub use ws::{Ws, accept_for};

pub const HTTP_ADDR_ENV: &str = "GATEWAY_HTTP_ADDR";

pub const DEFAULT_HTTP_ADDR: &str = "127.0.0.1:7880";

pub const AUTH_ADDR_ENV: &str = "AUTH_HTTP_ADDR";

pub const DEFAULT_AUTH_ADDR: &str = "127.0.0.1:7881";

pub const CA_ENV: &str = "SOCIAL_CA";

pub const ADMIN_ADDR_ENV: &str = "GATEWAY_ADMIN_ADDR";

pub const DEFAULT_ADMIN_ADDR: &str = "127.0.0.1:9880";

pub async fn connect(addr: &str, token: &str, attempts: u32) -> io::Result<Ws> {
    connect_with(&Dialer::plain(), addr, token, attempts).await
}

pub async fn connect_with(
    dialer: &Dialer,
    addr: &str,
    token: &str,
    attempts: u32,
) -> io::Result<Ws> {
    let path = format!("/ws?token={token}");
    let mut last = io::Error::other("шлюз не ответил");
    for _ in 0..attempts.max(1) {
        match Ws::connect_with(dialer, addr, &path).await {
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
    run_with(gateway, auth, Dialer::plain(), input, output).await
}

pub async fn run_with<R, W>(
    gateway: &str,
    auth: &str,
    dialer: Dialer,
    input: R,
    output: W,
) -> io::Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut client = client::Client::new(gateway, auth, dialer, output);
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
