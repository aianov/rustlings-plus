use std::collections::VecDeque;

use bytes::BytesMut;
use framing::{Encoder, Framed};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

use crate::codec::{RespCodec, RespError};
use crate::command::Cmd;
use crate::config::RedisConfig;
use crate::value::Value;

pub type Replies = Result<Vec<Value>, RespError>;

pub struct Request {
    pub commands: Vec<Cmd>,
    pub reply: oneshot::Sender<Replies>,
}

struct Waiting {
    left: usize,
    got: Vec<Value>,
    reply: oneshot::Sender<Replies>,
}

struct Link {
    reader: Framed<OwnedReadHalf, RespCodec>,
    writer: OwnedWriteHalf,
}

pub fn spawn(config: RedisConfig) -> mpsc::Sender<Request> {
    let (tx, rx) = mpsc::channel(256);
    tokio::spawn(run(config, rx));
    tx
}

async fn open(config: &RedisConfig) -> Result<Link, RespError> {
    let stream = timeout(config.connect_timeout, TcpStream::connect(&config.addr))
        .await
        .map_err(|_| RespError::Timeout)??;
    stream.set_nodelay(true)?;
    let (read, writer) = stream.into_split();
    Ok(Link {
        reader: Framed::new(read, RespCodec::default()),
        writer,
    })
}

async fn write(link: &mut Link, commands: &[Cmd]) -> Result<(), RespError> {
    let mut buf = BytesMut::new();
    let mut codec = RespCodec::default();
    for command in commands {
        codec.encode(command, &mut buf)?;
    }
    link.writer.write_all(&buf).await?;
    Ok(())
}

fn fail_all(waiting: &mut VecDeque<Waiting>) {
    for left in waiting.drain(..) {
        let _ = left.reply.send(Err(RespError::Closed));
    }
}

async fn run(config: RedisConfig, mut requests: mpsc::Receiver<Request>) {
    let mut link: Option<Link> = None;
    let mut waiting: VecDeque<Waiting> = VecDeque::new();
    loop {
        let Some(open_link) = link.as_mut() else {
            let Some(request) = requests.recv().await else {
                return;
            };
            match open(&config).await {
                Ok(mut fresh) => match write(&mut fresh, &request.commands).await {
                    Ok(()) => {
                        waiting.push_back(Waiting {
                            left: request.commands.len(),
                            got: Vec::new(),
                            reply: request.reply,
                        });
                        link = Some(fresh);
                    }
                    Err(error) => {
                        let _ = request.reply.send(Err(error));
                    }
                },
                Err(error) => {
                    let _ = request.reply.send(Err(error));
                }
            }
            continue;
        };
        tokio::select! {
            request = requests.recv() => {
                let Some(request) = request else {
                    return;
                };
                match write(open_link, &request.commands).await {
                    Ok(()) => waiting.push_back(Waiting {
                        left: request.commands.len(),
                        got: Vec::new(),
                        reply: request.reply,
                    }),
                    Err(error) => {
                        let _ = request.reply.send(Err(error));
                        fail_all(&mut waiting);
                        link = None;
                    }
                }
            }
            frame = open_link.reader.recv(), if !waiting.is_empty() => {
                match frame {
                    Ok(Some(value)) => {
                        let done = match waiting.front_mut() {
                            Some(front) => {
                                front.got.push(value);
                                front.left -= 1;
                                front.left == 0
                            }
                            None => false,
                        };
                        if done && let Some(front) = waiting.pop_front() {
                            let _ = front.reply.send(Ok(front.got));
                        }
                    }
                    Ok(None) | Err(..) => {
                        fail_all(&mut waiting);
                        link = None;
                    }
                }
            }
        }
    }
}
