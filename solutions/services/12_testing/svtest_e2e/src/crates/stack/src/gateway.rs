use crate::rpc::{Alive, Service, request_json};
use async_nats::Client;
use futures::StreamExt;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinSet;

pub struct Gateway {
    pub addr: SocketAddr,
    pub service: Service,
}

async fn auth(client: &Client, p: &str, token: &str) -> Option<u64> {
    let answer = request_json(client, &format!("{p}.auth.verify"), &json!({"token": token})).await.ok()?;
    answer["id"].as_u64()
}

async fn ask(client: &Client, p: &str, subject: &str, body: serde_json::Value) -> Result<serde_json::Value, String> {
    request_json(client, &format!("{p}.{subject}"), &body).await
}

async fn command(client: &Client, p: &str, line: &str) -> String {
    let mut parts = line.splitn(3, ' ');
    let (verb, first, rest) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    match verb {
        "REGISTER" => match ask(client, p, "auth.register", json!({"name": first, "password": rest})).await {
            Ok(v) if v["id"].is_u64() => "OK".to_string(),
            _ => "ERR exists".to_string(),
        },
        "LOGIN" => match ask(client, p, "auth.login", json!({"name": first, "password": rest})).await {
            Ok(v) if v["token"].is_string() => format!("TOKEN {}", v["token"].as_str().unwrap_or("")),
            _ => "ERR bad credentials".to_string(),
        },
        "FOLLOW" => match auth(client, p, first).await {
            None => "ERR unauthorized".to_string(),
            Some(who) => match ask(client, p, "users.follow", json!({"who": who, "whom": rest})).await {
                Ok(v) if v["ok"] == true => "OK".to_string(),
                _ => "ERR no such user".to_string(),
            },
        },
        "POST" => match ask(client, p, "posts.create", json!({"token": first, "text": rest})).await {
            Ok(v) if v["ok"] == true => "OK".to_string(),
            _ => "ERR unauthorized".to_string(),
        },
        "UNREAD" => match auth(client, p, first).await {
            None => "ERR unauthorized".to_string(),
            Some(id) => match ask(client, p, "notify.count", json!({"id": id})).await {
                Ok(v) => format!("UNREAD {}", v["n"].as_u64().unwrap_or(0)),
                Err(_) => "ERR unavailable".to_string(),
            },
        },
        _ => "ERR unknown command".to_string(),
    }
}

async fn handle(client: Client, p: String, stream: TcpStream, mut stop: watch::Receiver<bool>, live: Arc<AtomicUsize>) {
    let _alive = Alive::enter(&live);
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    let mut pushes: Option<async_nats::Subscriber> = None;
    loop {
        tokio::select! {
            _ = stop.changed() => return,
            push = async { match pushes.as_mut() { Some(sub) => sub.next().await, None => std::future::pending().await } } => {
                let Some(message) = push else { return };
                let text = String::from_utf8_lossy(&message.payload).to_string();
                if write.write_all(format!("NEW {text}\n").as_bytes()).await.is_err() {
                    return;
                }
            }
            line = lines.next_line() => {
                let Ok(Some(line)) = line else { return };
                let reply = if let Some(token) = line.strip_prefix("LISTEN ") {
                    match auth(&client, &p, token).await {
                        None => "ERR unauthorized".to_string(),
                        Some(id) => match client.subscribe(format!("{p}.push.{id}")).await {
                            Ok(sub) => {
                                crate::rpc::sync(&client).await;
                                pushes = Some(sub);
                                "OK".to_string()
                            }
                            Err(_) => "ERR unavailable".to_string(),
                        },
                    }
                } else {
                    command(&client, &p, &line).await
                };
                if write.write_all(format!("{reply}\n").as_bytes()).await.is_err() {
                    return;
                }
            }
        }
    }
}

pub async fn start(client: &Client, p: &str) -> Gateway {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("свободный порт");
    let addr = listener.local_addr().expect("адрес сокета");
    let (mut service, mut stop) = Service::new();
    let alive = Alive::enter(&service.live);
    let (client, p, live) = (client.clone(), p.to_string(), service.live.clone());
    service.tasks.push(tokio::spawn(async move {
        let _alive = alive;
        let mut connections = JoinSet::new();
        loop {
            tokio::select! {
                _ = stop.changed() => break,
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else { continue };
                    connections.spawn(handle(client.clone(), p.clone(), stream, stop.clone(), live.clone()));
                }
            }
        }
        drop(listener);
        while connections.join_next().await.is_some() {}
    }));
    Gateway { addr, service }
}
