use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::watch;
use tokio::task::{JoinHandle, JoinSet};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Identity, Server, ServerTlsConfig};
use tonic::{Request, Response, Status};
use tonic_health::server::health_reporter;
use tonic_health::ServingStatus;

pub mod pb {
    tonic::include_proto!("metrics.v1");
}
use pb::admin_server::{Admin, AdminServer};
use pb::{GetRequest, Kind, ListRequest, Metric, ResetReply, ResetRequest};

const MAX_NAMES: usize = 1000;
const MAX_NAME_LEN: usize = 100;
const HEAD_LIMIT: usize = 8192;
const HEAD_TIMEOUT: Duration = Duration::from_secs(2);
const GRACE: Duration = Duration::from_millis(100);

#[derive(Clone)]
pub struct Config {
    pub admin_token: String,
    pub cert_pem: String,
    pub key_pem: String,
}

pub struct Running {
    pub udp: SocketAddr,
    pub http: SocketAddr,
    pub grpc: SocketAddr,
    stop: watch::Sender<bool>,
    tasks: Vec<JoinHandle<()>>,
}

#[derive(Clone, Debug, PartialEq)]
enum Value {
    Counter(i64),
    Gauge(f64),
    Timer { count: u64, sum: f64 },
}

enum Sample {
    Count(i64),
    Gauge { value: f64, relative: bool },
    Time(f64),
}

#[derive(Default)]
struct Store {
    metrics: BTreeMap<String, Value>,
    bad_lines: u64,
    dropped_names: u64,
}

type Shared = Arc<Mutex<Store>>;

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= MAX_NAME_LEN && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
}

fn parse_line(line: &str) -> Option<(&str, Sample)> {
    let (name, rest) = line.split_once(':')?;
    if !valid_name(name) {
        return None;
    }
    let (value, kind) = rest.split_once('|')?;
    let sample = match kind {
        "c" => Sample::Count(value.parse().ok()?),
        "g" => {
            let parsed: f64 = value.parse().ok()?;
            if !parsed.is_finite() {
                return None;
            }
            Sample::Gauge { value: parsed, relative: value.starts_with('+') || value.starts_with('-') }
        }
        "ms" => {
            let parsed: f64 = value.parse().ok()?;
            if !parsed.is_finite() || parsed < 0.0 {
                return None;
            }
            Sample::Time(parsed)
        }
        _ => return None,
    };
    Some((name, sample))
}

impl Store {
    fn feed(&mut self, line: &str) {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            return;
        }
        let Some((name, sample)) = parse_line(line) else {
            self.bad_lines += 1;
            return;
        };
        if !self.metrics.contains_key(name) && self.metrics.len() >= MAX_NAMES {
            self.dropped_names += 1;
            return;
        }
        use std::collections::btree_map::Entry;
        match (self.metrics.entry(name.to_string()), sample) {
            (Entry::Vacant(slot), Sample::Count(n)) => {
                slot.insert(Value::Counter(n));
            }
            (Entry::Vacant(slot), Sample::Gauge { value, .. }) => {
                slot.insert(Value::Gauge(value));
            }
            (Entry::Vacant(slot), Sample::Time(v)) => {
                slot.insert(Value::Timer { count: 1, sum: v });
            }
            (Entry::Occupied(mut slot), sample) => match (slot.get_mut(), sample) {
                (Value::Counter(c), Sample::Count(n)) => *c = c.saturating_add(n),
                (Value::Gauge(g), Sample::Gauge { value, relative }) => {
                    if relative {
                        *g += value;
                    } else {
                        *g = value;
                    }
                }
                (Value::Timer { count, sum }, Sample::Time(v)) => {
                    *count += 1;
                    *sum += v;
                }
                _ => self.bad_lines += 1,
            },
        }
    }

    fn render(&self) -> String {
        let mut out = String::new();
        for (name, value) in &self.metrics {
            let prom = name.replace('.', "_");
            let _ = match value {
                Value::Counter(n) => writeln!(out, "# TYPE {prom} counter\n{prom}_total {n}"),
                Value::Gauge(g) => writeln!(out, "# TYPE {prom} gauge\n{prom} {g}"),
                Value::Timer { count, sum } => writeln!(out, "# TYPE {prom} summary\n{prom}_count {count}\n{prom}_sum {sum}"),
            };
        }
        let _ = writeln!(out, "# TYPE statsd_bad_lines_total counter\nstatsd_bad_lines_total {}", self.bad_lines);
        let _ = writeln!(out, "# TYPE statsd_dropped_names_total counter\nstatsd_dropped_names_total {}", self.dropped_names);
        out
    }
}

fn to_pb(name: &str, value: &Value) -> Metric {
    match value {
        Value::Counter(n) => Metric { name: name.to_string(), kind: Kind::Counter as i32, value: *n as f64, count: 0 },
        Value::Gauge(g) => Metric { name: name.to_string(), kind: Kind::Gauge as i32, value: *g, count: 0 },
        Value::Timer { count, sum } => Metric { name: name.to_string(), kind: Kind::Timer as i32, value: *sum, count: *count },
    }
}

async fn udp_loop(socket: UdpSocket, store: Shared, mut stop: watch::Receiver<bool>) {
    let mut buf = vec![0u8; 2048];
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            received = socket.recv_from(&mut buf) => {
                let Ok((n, _)) = received else { continue };
                let text = String::from_utf8_lossy(&buf[..n]).into_owned();
                let mut store = store.lock().unwrap();
                for line in text.split('\n') {
                    store.feed(line);
                }
            }
        }
    }
}

enum Head {
    Complete(String),
    TooLarge,
    Closed,
}

async fn read_head(tcp: &mut TcpStream) -> io::Result<Head> {
    let mut data = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            return Ok(Head::Complete(String::from_utf8_lossy(&data[..pos]).into_owned()));
        }
        if data.len() > HEAD_LIMIT {
            return Ok(Head::TooLarge);
        }
        let n = tcp.read(&mut chunk).await?;
        if n == 0 {
            return Ok(Head::Closed);
        }
        data.extend_from_slice(&chunk[..n]);
    }
}

async fn respond(tcp: &mut TcpStream, status: &str, extra: &str, body: &str) {
    let text = format!("HTTP/1.1 {status}\r\nContent-Type: text/plain; version=0.0.4; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}", body.len());
    let _ = tcp.write_all(text.as_bytes()).await;
    let _ = tcp.shutdown().await;
}

async fn drain(tcp: &mut TcpStream) {
    let mut sink = [0u8; 4096];
    let _ = tokio::time::timeout(Duration::from_millis(300), async {
        while matches!(tcp.read(&mut sink).await, Ok(n) if n > 0) {}
    })
    .await;
}

async fn http_conn(mut tcp: TcpStream, store: Shared) {
    let head = match tokio::time::timeout(HEAD_TIMEOUT, read_head(&mut tcp)).await {
        Err(_) => {
            respond(&mut tcp, "408 Request Timeout", "", "timeout\n").await;
            return;
        }
        Ok(Err(_)) | Ok(Ok(Head::Closed)) => return,
        Ok(Ok(Head::TooLarge)) => {
            respond(&mut tcp, "431 Request Header Fields Too Large", "", "head too large\n").await;
            drain(&mut tcp).await;
            return;
        }
        Ok(Ok(Head::Complete(head))) => head,
    };
    let request_line = head.lines().next().unwrap_or("");
    let parts: Vec<&str> = request_line.split(' ').collect();
    let [method, target, version] = parts[..] else {
        respond(&mut tcp, "400 Bad Request", "", "bad request\n").await;
        return;
    };
    if version != "HTTP/1.1" && version != "HTTP/1.0" {
        respond(&mut tcp, "400 Bad Request", "", "bad request\n").await;
        return;
    }
    let path = target.split('?').next().unwrap_or("");
    let known = path == "/metrics" || path == "/healthz";
    if !known {
        respond(&mut tcp, "404 Not Found", "", "not found\n").await;
    } else if method != "GET" {
        respond(&mut tcp, "405 Method Not Allowed", "Allow: GET\r\n", "method not allowed\n").await;
    } else if path == "/healthz" {
        respond(&mut tcp, "200 OK", "", "ok\n").await;
    } else {
        let body = store.lock().unwrap().render();
        respond(&mut tcp, "200 OK", "", &body).await;
    }
}

async fn http_loop(listener: TcpListener, store: Shared, mut stop: watch::Receiver<bool>) {
    let mut conns = JoinSet::new();
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            Some(_) = conns.join_next(), if !conns.is_empty() => {}
            accepted = listener.accept() => {
                let Ok((tcp, _)) = accepted else { continue };
                conns.spawn(http_conn(tcp, store.clone()));
            }
        }
    }
    drop(listener);
    let _ = tokio::time::timeout(Duration::from_secs(3), async { while conns.join_next().await.is_some() {} }).await;
}

struct AdminSvc {
    store: Shared,
}

#[tonic::async_trait]
impl Admin for AdminSvc {
    async fn get(&self, request: Request<GetRequest>) -> Result<Response<Metric>, Status> {
        let name = request.into_inner().name;
        if !valid_name(&name) {
            return Err(Status::invalid_argument("name: допустимы буквы, цифры, _ и . (до 100 символов)"));
        }
        let store = self.store.lock().unwrap();
        store.metrics.get(&name).map(|v| Response::new(to_pb(&name, v))).ok_or_else(|| Status::not_found(format!("метрики {name} нет")))
    }

    type ListStream = tokio_stream::Iter<std::vec::IntoIter<Result<Metric, Status>>>;

    async fn list(&self, request: Request<ListRequest>) -> Result<Response<Self::ListStream>, Status> {
        let prefix = request.into_inner().prefix;
        let items: Vec<Result<Metric, Status>> = self.store.lock().unwrap().metrics.iter().filter(|(n, _)| n.starts_with(&prefix)).map(|(n, v)| Ok(to_pb(n, v))).collect();
        Ok(Response::new(tokio_stream::iter(items)))
    }

    async fn reset(&self, request: Request<ResetRequest>) -> Result<Response<ResetReply>, Status> {
        let name = request.into_inner().name;
        if !valid_name(&name) {
            return Err(Status::invalid_argument("name: допустимы буквы, цифры, _ и . (до 100 символов)"));
        }
        match self.store.lock().unwrap().metrics.remove(&name) {
            Some(_) => Ok(Response::new(ResetReply {})),
            None => Err(Status::not_found(format!("метрики {name} нет"))),
        }
    }
}

async fn grpc_loop(listener: TcpListener, store: Shared, config: Config, mut stop: watch::Receiver<bool>) {
    let (reporter, health) = health_reporter();
    reporter.set_service_status("metrics.v1.Admin", ServingStatus::Serving).await;
    let token = format!("Bearer {}", config.admin_token);
    let auth = move |request: Request<()>| -> Result<Request<()>, Status> {
        match request.metadata().get("authorization").and_then(|v| v.to_str().ok()) {
            Some(value) if value == token => Ok(request),
            _ => Err(Status::unauthenticated("нужен токен администратора")),
        }
    };
    let signal = async move {
        let _ = stop.changed().await;
        reporter.set_service_status("metrics.v1.Admin", ServingStatus::NotServing).await;
        tokio::time::sleep(GRACE).await;
    };
    let tls = ServerTlsConfig::new().identity(Identity::from_pem(config.cert_pem, config.key_pem));
    let Ok(mut server) = Server::builder().tls_config(tls) else { return };
    let _ = server
        .add_service(health)
        .add_service(AdminServer::with_interceptor(AdminSvc { store }, auth))
        .serve_with_incoming_shutdown(TcpListenerStream::new(listener), signal)
        .await;
}

pub async fn start(config: Config) -> io::Result<Running> {
    let udp = UdpSocket::bind("127.0.0.1:0").await?;
    let http = TcpListener::bind("127.0.0.1:0").await?;
    let grpc = TcpListener::bind("127.0.0.1:0").await?;
    let addrs = (udp.local_addr()?, http.local_addr()?, grpc.local_addr()?);
    let store: Shared = Arc::default();
    let (stop, rx) = watch::channel(false);
    let tasks = vec![
        tokio::spawn(udp_loop(udp, store.clone(), rx.clone())),
        tokio::spawn(http_loop(http, store.clone(), rx.clone())),
        tokio::spawn(grpc_loop(grpc, store, config, rx)),
    ];
    Ok(Running { udp: addrs.0, http: addrs.1, grpc: addrs.2, stop, tasks })
}

impl Running {
    pub async fn shutdown(self) {
        let _ = self.stop.send(true);
        for task in self.tasks {
            let _ = tokio::time::timeout(Duration::from_secs(5), task).await;
        }
    }
}
