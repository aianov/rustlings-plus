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

pub async fn connect(addr: &str, attempts: u32) -> io::Result<Ws> {
    let mut last = io::Error::other("шлюз не ответил");
    for _ in 0..attempts.max(1) {
        match Ws::connect(addr, "/ws").await {
            Ok(stream) => return Ok(stream),
            Err(error) => last = error,
        }
        sleep(Duration::from_millis(100)).await;
    }
    Err(last)
}

pub async fn run<R, W>(addr: &str, input: R, output: W) -> io::Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let ws = connect(addr, 50).await?;
    let mut client = client::Client::new(ws, addr, output);
    client
        .say(&format!(
            "подключено к ws://{addr}/ws, help покажет команды"
        ))
        .await?;
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
            incoming = client.incoming() => match incoming? {
                Some(raw) => client.unsolicited(&raw).await?,
                None => {
                    client.say("шлюз закрыл соединение").await?;
                    break;
                }
            },
        }
    }
    Ok(())
}
