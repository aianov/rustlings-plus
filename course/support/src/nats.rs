//! Помощники для тестов с NATS (фича `nats`).
//!
//! NATS курса поднят с JetStream, и все тесты ходят в один сервер параллельно. Чтобы не слышать друг
//! друга, каждый тест берёт свой `Scope`: уникальный префикс, под которым живут все его темы, стримы
//! и durable-имена. Подписки в core NATS ничего не хранят, поэтому чистить после теста нечего.
//!
//! API:
//!
//! - `url()`, `DEFAULT_URL`: адрес из `RUSTLINGS_NATS_URL` или значение по умолчанию.
//! - `connect().await -> Client`: подключение с настройками по умолчанию, паника с понятным текстом,
//!   если сервер не отвечает. `connect_with(options).await` то же, но с вашими `ConnectOptions`
//!   (например, `subscription_capacity` или `event_callback`). `connect_to(url, options).await` для
//!   нестандартного адреса (например, адреса `Proxy`).
//! - `Scope::new()`: уникальный префикс теста вида `t_<pid>_<время>_<n>`; это один токен темы
//!   (без точек). `scope.prefix()`, `scope.subject("post.created")` даёт `<prefix>.post.created`,
//!   `scope.wildcard()` даёт `<prefix>.>`, `scope.name("durable")` даёт имя без точек для
//!   стримов, консьюмеров и ключей вида `<prefix>_durable`.
//! - `sync(&client).await`: круговой рейс до сервера (публикация в собственный inbox и ожидание ответа).
//!   Когда он вернулся, брокер точно обработал всё, что это соединение отправило раньше. `Client::flush`
//!   для этого не годится: он лишь дописывает буфер в сокет.
//! - `listen(&client, subject).await -> Subscriber`: подписка и `sync`. После возврата сервер уже
//!   зарегистрировал подписку, поэтому публикация с другого соединения не потеряется.
//!   `listen_queue(&client, subject, group).await` то же для queue group.
//! - `recv(&mut sub, within).await -> Message`: следующее сообщение или паника, если за `within`
//!   ничего не пришло. `try_recv(..) -> Option<Message>` то же без паники.
//!   `recv_n(&mut sub, n, within).await -> Vec<Message>`: ровно `n` сообщений, `within` на все вместе.
//!   `expect_silence(&mut sub, window).await`: паника, если за `window` пришло хоть что-то.
//! - JetStream: `js(&client) -> jetstream::Context`. Стримы и ключи KV живут на брокере после теста, поэтому
//!   `scope.track(stream_name)` и `scope.track_bucket(bucket)` записывают, что удалить: когда `Scope`
//!   уничтожается (в том числе после паники в `assert!`), все записанные стримы удаляются. Имена стримов,
//!   консьюмеров и бакетов бери из `scope.name("тег")`, темы из `scope.subject(..)`.
//! - `wait_until(что_ждём, срок, || async { условие }).await`: опрашивает условие каждые 20 мс, паника с
//!   описанием, если за срок оно не стало истинным. Для проверок вида «в стриме стало 0 сообщений».
//! - `text(&msg) -> String`: payload как строка (для сообщений, где он текст).
//! - `Proxy::start().await`: TCP-прокси между тестом и сервером NATS. `proxy.url()` отдаёт адрес
//!   для `connect_to`, `proxy.cut()` рвёт все соединения и сбрасывает новые, пока не вызван
//!   `proxy.restore()`; `proxy.accepted()` считает принятые соединения. Так в тесте
//!   воспроизводится обрыв связи с брокером, не трогая сам сервер. `proxy.set_latency(d)` добавляет
//!   задержку `d` каждому куску байтов от клиента к серверу: подписка клиента доходит до брокера
//!   с опозданием, и тест видит гонку, которая на локальной машине обычно незаметна.
//!   Прокси останавливается при `drop`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use async_nats::{Client, ConnectOptions, Message, Subscriber};
use futures::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::time::Instant;

pub const DEFAULT_URL: &str = "nats://127.0.0.1:54222";

pub fn url() -> String {
    crate::env_or("RUSTLINGS_NATS_URL", DEFAULT_URL)
}

pub async fn connect() -> Client {
    connect_with(ConnectOptions::new()).await
}

pub async fn connect_with(options: ConnectOptions) -> Client {
    connect_to(&url(), options).await
}

pub async fn connect_to(url: &str, options: ConnectOptions) -> Client {
    options
        .connect(url)
        .await
        .unwrap_or_else(|e| panic!("NATS курса не отвечает по адресу {url}: {e}. Запусти `docker compose up -d`."))
}

pub struct Scope {
    prefix: String,
    tracked: std::sync::Mutex<Vec<String>>,
}

impl Scope {
    pub fn new() -> Scope {
        Scope { prefix: crate::unique_name("t"), tracked: std::sync::Mutex::new(Vec::new()) }
    }

    pub fn track(&self, stream: &str) {
        self.tracked.lock().unwrap_or_else(|p| p.into_inner()).push(stream.to_string());
    }

    pub fn track_bucket(&self, bucket: &str) {
        self.track(&format!("KV_{bucket}"));
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn subject(&self, tail: &str) -> String {
        format!("{}.{tail}", self.prefix)
    }

    pub fn wildcard(&self) -> String {
        format!("{}.>", self.prefix)
    }

    pub fn name(&self, tag: &str) -> String {
        format!("{}_{tag}", self.prefix)
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        let names = std::mem::take(&mut *self.tracked.lock().unwrap_or_else(|p| p.into_inner()));
        if names.is_empty() {
            return;
        }
        let url = url();
        let cleanup = std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
            runtime.block_on(async {
                let Ok(client) = async_nats::connect(url.as_str()).await else { return };
                let jetstream = async_nats::jetstream::new(client.clone());
                for name in names {
                    jetstream.delete_stream(&name).await.ok();
                }
                client.flush().await.ok();
            });
        });
        cleanup.join().ok();
    }
}

pub fn js(client: &Client) -> async_nats::jetstream::Context {
    async_nats::jetstream::new(client.clone())
}

pub async fn wait_until<F, Fut>(what: &str, within: Duration, mut condition: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = Instant::now() + within;
    while !condition().await {
        if Instant::now() >= deadline {
            panic!("за {within:?} не наступило: {what}");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

impl Default for Scope {
    fn default() -> Scope {
        Scope::new()
    }
}

pub async fn sync(client: &Client) {
    let inbox = client.new_inbox();
    let mut probe = client.subscribe(inbox.clone()).await.unwrap_or_else(|e| panic!("sync: не удалось подписаться: {e}"));
    client.publish(inbox, "".into()).await.unwrap_or_else(|e| panic!("sync: не удалось опубликовать: {e}"));
    if tokio::time::timeout(Duration::from_secs(5), probe.next()).await.is_err() {
        panic!("sync: сервер не ответил за 5 секунд");
    }
}

pub async fn listen(client: &Client, subject: impl async_nats::subject::ToSubject) -> Subscriber {
    let sub = client.subscribe(subject).await.unwrap_or_else(|e| panic!("не удалось подписаться: {e}"));
    sync(client).await;
    sub
}

pub async fn listen_queue(client: &Client, subject: impl async_nats::subject::ToSubject, group: &str) -> Subscriber {
    let sub = client
        .queue_subscribe(subject, group.to_string())
        .await
        .unwrap_or_else(|e| panic!("не удалось подписаться в группу {group}: {e}"));
    sync(client).await;
    sub
}

pub async fn try_recv(sub: &mut Subscriber, within: Duration) -> Option<Message> {
    tokio::time::timeout(within, sub.next()).await.ok().flatten()
}

pub async fn recv(sub: &mut Subscriber, within: Duration) -> Message {
    match tokio::time::timeout(within, sub.next()).await {
        Ok(Some(message)) => message,
        Ok(None) => panic!("подписка закрылась, а сообщения ждали"),
        Err(_) => panic!("за {within:?} не пришло ни одного сообщения"),
    }
}

pub async fn recv_n(sub: &mut Subscriber, n: usize, within: Duration) -> Vec<Message> {
    let deadline = Instant::now() + within;
    let mut got = Vec::with_capacity(n);
    while got.len() < n {
        match tokio::time::timeout_at(deadline, sub.next()).await {
            Ok(Some(message)) => got.push(message),
            Ok(None) => panic!("подписка закрылась: пришло {} сообщений из {n}", got.len()),
            Err(_) => panic!("за {within:?} пришло {} сообщений из {n}", got.len()),
        }
    }
    got
}

pub async fn expect_silence(sub: &mut Subscriber, window: Duration) {
    if let Some(message) = try_recv(sub, window).await {
        panic!("ждали тишину, а пришло сообщение на {}: {:?}", message.subject, text(&message));
    }
}

pub fn text(message: &Message) -> String {
    String::from_utf8_lossy(&message.payload).into_owned()
}

pub struct Proxy {
    addr: SocketAddr,
    cut: Arc<AtomicBool>,
    kill: broadcast::Sender<()>,
    accepted: Arc<AtomicUsize>,
    latency_ms: Arc<AtomicU64>,
    task: JoinHandle<()>,
}

impl Proxy {
    pub async fn start() -> Proxy {
        let upstream = url();
        let upstream = upstream.strip_prefix("nats://").unwrap_or(&upstream).to_string();
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт для прокси");
        let addr = listener.local_addr().expect("адрес прокси");
        let cut = Arc::new(AtomicBool::new(false));
        let accepted = Arc::new(AtomicUsize::new(0));
        let latency_ms = Arc::new(AtomicU64::new(0));
        let (kill, _) = broadcast::channel(1);
        let task = tokio::spawn({
            let (cut, accepted, kill, latency_ms) = (cut.clone(), accepted.clone(), kill.clone(), latency_ms.clone());
            async move {
                while let Ok((client, _)) = listener.accept().await {
                    if cut.load(Ordering::SeqCst) {
                        continue;
                    }
                    accepted.fetch_add(1, Ordering::SeqCst);
                    let mut stop = kill.subscribe();
                    let upstream = upstream.clone();
                    let latency_ms = latency_ms.clone();
                    tokio::spawn(async move {
                        let Ok(server) = TcpStream::connect(&upstream).await else { return };
                        let (mut from_client, mut to_client) = client.into_split();
                        let (mut from_server, mut to_server) = server.into_split();
                        let up = async move {
                            let mut chunk = [0u8; 16 * 1024];
                            loop {
                                let n = match from_client.read(&mut chunk).await {
                                    Ok(0) | Err(_) => break,
                                    Ok(n) => n,
                                };
                                let delay = latency_ms.load(Ordering::SeqCst);
                                if delay > 0 {
                                    tokio::time::sleep(Duration::from_millis(delay)).await;
                                }
                                if to_server.write_all(&chunk[..n]).await.is_err() {
                                    break;
                                }
                            }
                        };
                        let down = async move {
                            tokio::io::copy(&mut from_server, &mut to_client).await.ok();
                        };
                        tokio::select! {
                            _ = up => {}
                            _ = down => {}
                            _ = stop.recv() => {}
                        }
                    });
                }
            }
        });
        Proxy { addr, cut, kill, accepted, latency_ms, task }
    }

    pub fn url(&self) -> String {
        format!("nats://{}", self.addr)
    }

    pub fn cut(&self) {
        self.cut.store(true, Ordering::SeqCst);
        self.kill.send(()).ok();
    }

    pub fn restore(&self) {
        self.cut.store(false, Ordering::SeqCst);
    }

    pub fn set_latency(&self, delay: Duration) {
        self.latency_ms.store(delay.as_millis() as u64, Ordering::SeqCst);
    }

    pub fn accepted(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.task.abort();
        self.kill.send(()).ok();
    }
}
