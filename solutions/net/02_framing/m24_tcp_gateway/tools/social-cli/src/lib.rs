mod client;
mod input;
mod render;

use std::io;
use std::time::Duration;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite};
use tokio::net::TcpStream;
use tokio::time::sleep;

pub use input::{Command, HELP, parse};

pub const ADDR_ENV: &str = "GATEWAY_ADDR";

pub const DEFAULT_ADDR: &str = "127.0.0.1:7878";

pub async fn connect(addr: &str, attempts: u32) -> io::Result<TcpStream> {
    let mut last = io::Error::other("шлюз не ответил");
    for _ in 0..attempts.max(1) {
        match TcpStream::connect(addr).await {
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
    let stream = connect(addr, 50).await?;
    let mut client = client::Client::new(stream, output);
    client
        .say(&format!("подключено к {addr}, help покажет команды"))
        .await?;
    let mut lines = input.lines();
    loop {
        tokio::select! {
            line = lines.next_line() => match line? {
                Some(line) => {
                    if !client.command(&line).await? {
                        break;
                    }
                }
                None => break,
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
