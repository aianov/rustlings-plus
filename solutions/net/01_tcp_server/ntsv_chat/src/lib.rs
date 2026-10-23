use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, watch};
use tokio::task::JoinSet;

const QUEUE: usize = 256;
const MAX_NICK: usize = 16;

type Users = Arc<Mutex<HashMap<String, mpsc::Sender<String>>>>;

pub async fn run(listener: TcpListener, max_connections: usize, shutdown: impl Future<Output = ()>) -> io::Result<()> {
    tokio::pin!(shutdown);
    let users: Users = Arc::default();
    let permits = Arc::new(Semaphore::new(max_connections));
    let (stop_tx, stop_rx) = watch::channel(false);
    let mut conns = JoinSet::new();
    loop {
        while conns.try_join_next().is_some() {}
        let permit = tokio::select! {
            _ = &mut shutdown => break,
            permit = permits.clone().acquire_owned() => permit.expect("семафор не закрывается"),
        };
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    conns.spawn(connection(stream, users.clone(), stop_rx.clone(), permit));
                }
                Err(e) if is_temporary(&e) => tokio::time::sleep(Duration::from_millis(50)).await,
                Err(e) => return Err(e),
            },
        }
    }
    drop(listener);
    let _ = stop_tx.send(true);
    while conns.join_next().await.is_some() {}
    Ok(())
}

fn is_temporary(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(23 | 24))
        || matches!(e.kind(), io::ErrorKind::ConnectionAborted | io::ErrorKind::ConnectionReset | io::ErrorKind::Interrupted)
}

async fn connection(stream: TcpStream, users: Users, mut stop: watch::Receiver<bool>, _permit: OwnedSemaphorePermit) {
    let (reader, mut writer) = stream.into_split();
    let (tx, mut rx) = mpsc::channel::<String>(QUEUE);
    let writer_task = tokio::spawn(async move {
        while let Some(mut line) = rx.recv().await {
            line.push('\n');
            if writer.write_all(line.as_bytes()).await.is_err() {
                return;
            }
        }
        let _ = writer.shutdown().await;
    });
    let nick = if *stop.borrow() { None } else { session(reader, &tx, &users, &mut stop).await };
    if let Some(nick) = nick {
        let mut map = users.lock().unwrap();
        map.remove(&nick);
        if !*stop.borrow() {
            broadcast(&map, &format!("LEAVE {nick}"), None);
        }
    }
    drop(tx);
    let _ = writer_task.await;
}

async fn session(reader: OwnedReadHalf, tx: &mpsc::Sender<String>, users: &Users, stop: &mut watch::Receiver<bool>) -> Option<String> {
    let mut lines = BufReader::new(reader).lines();
    let mut nick: Option<String> = None;
    loop {
        let line = tokio::select! {
            line = lines.next_line() => match line {
                Ok(Some(line)) => line,
                _ => break,
            },
            _ = stop.changed() => {
                let _ = tx.send("BYE".to_owned()).await;
                break;
            }
        };
        if line == "QUIT" {
            let _ = tx.send("BYE".to_owned()).await;
            break;
        }
        if let Some(wanted) = line.strip_prefix("NICK ") {
            if nick.is_some() {
                let _ = tx.send("ERR already_named".to_owned()).await;
            } else {
                nick = register(wanted, tx, users);
            }
            continue;
        }
        if let Some(text) = line.strip_prefix("MSG ") {
            match &nick {
                Some(name) => broadcast(&users.lock().unwrap(), &format!("FROM {name} {text}"), Some(name)),
                None => {
                    let _ = tx.send("ERR nick_required".to_owned()).await;
                }
            }
            continue;
        }
        let _ = tx.send("ERR unknown".to_owned()).await;
    }
    nick
}

fn register(wanted: &str, tx: &mpsc::Sender<String>, users: &Users) -> Option<String> {
    let valid = !wanted.is_empty() && wanted.len() <= MAX_NICK && wanted.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid {
        let _ = tx.try_send("ERR bad_nick".to_owned());
        return None;
    }
    let mut map = users.lock().unwrap();
    if map.contains_key(wanted) {
        let _ = tx.try_send("ERR taken".to_owned());
        return None;
    }
    let _ = tx.try_send("OK".to_owned());
    map.insert(wanted.to_owned(), tx.clone());
    broadcast(&map, &format!("JOIN {wanted}"), Some(wanted));
    Some(wanted.to_owned())
}

fn broadcast(map: &HashMap<String, mpsc::Sender<String>>, line: &str, except: Option<&str>) {
    for (name, sender) in map {
        if Some(name.as_str()) != except {
            let _ = sender.try_send(line.to_owned());
        }
    }
}
