use metricsd::pb::admin_client::AdminClient;
use metricsd::pb::{GetRequest, Kind, ListRequest, ResetRequest};
use metricsd::{start, Config, Running};
use std::future::Future;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tonic::metadata::MetadataValue;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint};
use tonic::{Code, Request};
use tonic_health::pb::health_client::HealthClient;
use tonic_health::pb::HealthCheckRequest;

const TOKEN: &str = "s3cret-admin-token";

struct Pki {
    ca_pem: String,
    cert_pem: String,
    key_pem: String,
}

fn pki() -> Pki {
    use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, KeyPair, KeyUsagePurpose};
    let mut ca_params = CertificateParams::new(Vec::new()).unwrap();
    ca_params.distinguished_name.push(DnType::CommonName, "exam CA");
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign];
    let ca_key = KeyPair::generate().unwrap();
    let ca = ca_params.self_signed(&ca_key).unwrap();
    let params = CertificateParams::new(vec!["localhost".to_string(), "127.0.0.1".to_string()]).unwrap();
    let key = KeyPair::generate().unwrap();
    let cert = params.signed_by(&key, &ca, &ca_key).unwrap();
    Pki { ca_pem: ca.pem(), cert_pem: cert.pem(), key_pem: key.serialize_pem() }
}

struct Env {
    run: Running,
    ca_pem: String,
}

async fn boot() -> Env {
    let p = pki();
    let run = start(Config { admin_token: TOKEN.to_string(), cert_pem: p.cert_pem, key_pem: p.key_pem }).await.expect("запуск сервиса");
    Env { run, ca_pem: p.ca_pem }
}

async fn within<T>(what: &str, f: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(8), f).await.unwrap_or_else(|_| panic!("{what}: не дождались за 8 секунд"))
}

async fn udp(env: &Env, text: &str) {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket.send_to(text.as_bytes(), env.run.udp).await.unwrap();
}

struct Reply {
    status: u16,
    head: String,
    body: String,
}

async fn http_raw(env: &Env, request: &[u8]) -> Reply {
    let mut tcp = TcpStream::connect(env.run.http).await.unwrap();
    tcp.write_all(request).await.unwrap();
    read_reply(&mut tcp).await
}

async fn read_reply(tcp: &mut TcpStream) -> Reply {
    let mut raw = Vec::new();
    let _ = within("чтение ответа", tcp.read_to_end(&mut raw)).await;
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head.split(' ').nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    Reply { status, head: head.to_string(), body: body.to_string() }
}

async fn get(env: &Env, path: &str) -> Reply {
    http_raw(env, format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes()).await
}

async fn eventually(env: &Env, want: &str) -> String {
    let started = Instant::now();
    loop {
        let body = get(env, "/metrics").await.body;
        if body.contains(want) {
            return body;
        }
        assert!(started.elapsed() < Duration::from_secs(4), "за 4 секунды в /metrics не появилось {want:?}; последний ответ:\n{body}");
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
}

#[allow(clippy::type_complexity)]
fn admin(env: &Env, token: Option<&str>) -> AdminClient<tonic::service::interceptor::InterceptedService<Channel, impl FnMut(Request<()>) -> Result<Request<()>, tonic::Status> + use<>>> {
    let tls = ClientTlsConfig::new().ca_certificate(Certificate::from_pem(&env.ca_pem)).domain_name("localhost");
    let channel = Endpoint::from_shared(format!("https://{}", env.run.grpc)).unwrap().tls_config(tls).unwrap().connect_lazy();
    let value = token.map(|t| MetadataValue::try_from(format!("Bearer {t}")).unwrap());
    AdminClient::with_interceptor(channel, move |mut req: Request<()>| {
        if let Some(v) = &value {
            req.metadata_mut().insert("authorization", v.clone());
        }
        Ok(req)
    })
}

#[tokio::test]
async fn udp_to_prometheus_text() {
    let env = boot().await;
    udp(&env, "web.hits:3|c\nmem.free:512|g\nreq.time:100|ms\nweb.hits:4|c\nreq.time:250.5|ms").await;
    let body = eventually(&env, "req_time_count 2").await;
    let want = "\
# TYPE mem_free gauge
mem_free 512
# TYPE req_time summary
req_time_count 2
req_time_sum 350.5
# TYPE web_hits counter
web_hits_total 7
# TYPE statsd_bad_lines_total counter
statsd_bad_lines_total 0
# TYPE statsd_dropped_names_total counter
statsd_dropped_names_total 0
";
    assert_eq!(body, want, "тело /metrics: метрики по алфавиту, точки в именах превращаются в _, в конце служебные счётчики");
    let reply = get(&env, "/metrics").await;
    assert!(reply.head.starts_with("HTTP/1.1 200"), "{}", reply.head);
    assert!(reply.head.to_ascii_lowercase().contains("content-type: text/plain"), "нужен Content-Type text/plain: {}", reply.head);
    assert!(reply.head.contains(&format!("Content-Length: {}", reply.body.len())), "Content-Length должен совпадать с телом: {}", reply.head);
    env.run.shutdown().await;
}

#[tokio::test]
async fn bad_lines_are_counted_and_good_ones_still_applied() {
    let env = boot().await;
    let datagram = "ok.one:1|c\n\nno_colon\nbad type:1|c\nx:abc|c\nx:1|q\n:5|c\nneg.timer:-5|ms\nnan:NaN|g\nok.two:2|c\nlong:1|c|@0.5\n\r\nok.one:1|c\r\n";
    udp(&env, datagram).await;
    let body = eventually(&env, "ok_two_total 2").await;
    assert!(body.contains("ok_one_total 2"), "обе хорошие строки ok.one учитываются (в том числе с \\r на конце):\n{body}");
    assert!(body.contains("statsd_bad_lines_total 8"), "плохих строк восемь (пустые не считаются):\n{body}");
    udp(&env, "ok.one:1|g").await;
    eventually(&env, "statsd_bad_lines_total 9").await;
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket.send_to(&[0xff, 0xfe, b':', b'1', b'|', b'c'], env.run.udp).await.unwrap();
    eventually(&env, "statsd_bad_lines_total 10").await;
    env.run.shutdown().await;
}

#[tokio::test]
async fn gauges_support_absolute_and_relative_values() {
    let env = boot().await;
    for line in ["g:10|g", "g:+5|g", "g:-3|g"] {
        udp(&env, line).await;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    eventually(&env, "g 12\n").await;
    udp(&env, "g:7|g").await;
    eventually(&env, "g 7\n").await;
    udp(&env, "fresh:-4|g").await;
    eventually(&env, "fresh -4\n").await;
    env.run.shutdown().await;
}

#[tokio::test]
async fn many_senders_lose_nothing() {
    let env = boot().await;
    let target = env.run.udp;
    let mut tasks = Vec::new();
    for _ in 0..4 {
        tasks.push(tokio::spawn(async move {
            let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            for i in 0..250 {
                socket.send_to(b"load.hits:1|c", target).await.unwrap();
                if i % 25 == 24 {
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
            }
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    eventually(&env, "load_hits_total 1000").await;
    env.run.shutdown().await;
}

#[tokio::test]
async fn number_of_names_is_capped() {
    let env = boot().await;
    for chunk in 0..15 {
        let text: String = (0..100).map(|i| format!("n{}:1|c\n", chunk * 100 + i)).collect();
        udp(&env, &text).await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let body = eventually(&env, "statsd_dropped_names_total 500").await;
    assert_eq!(body.matches("# TYPE n").count(), 1000, "хранится ровно тысяча имён");
    udp(&env, "n0:5|c").await;
    eventually(&env, "n0_total 6").await;
    env.run.shutdown().await;
}

#[tokio::test]
async fn http_edge_cases() {
    let env = boot().await;
    assert_eq!(get(&env, "/healthz").await.body, "ok\n");
    assert_eq!(get(&env, "/metrics?debug=1").await.status, 200, "строка запроса не мешает пути");
    assert_eq!(get(&env, "/nope").await.status, 404);
    let post = http_raw(&env, b"POST /metrics HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\n\r\n").await;
    assert_eq!(post.status, 405);
    assert!(post.head.contains("Allow: GET"), "405 обязан называть разрешённый метод: {}", post.head);
    assert_eq!(http_raw(&env, b"GARBAGE\r\n\r\n").await.status, 400);
    assert_eq!(http_raw(&env, b"GET /metrics HTTP/9.9\r\n\r\n").await.status, 400);
    let mut huge = b"GET /metrics HTTP/1.1\r\n".to_vec();
    for _ in 0..300 {
        huge.extend_from_slice(b"X-Pad: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\n");
    }
    huge.extend_from_slice(b"\r\n");
    assert_eq!(http_raw(&env, &huge).await.status, 431, "слишком длинная голова запроса");
    env.run.shutdown().await;
}

#[tokio::test]
async fn silent_http_client_is_dropped_and_does_not_block_others() {
    let env = boot().await;
    let mut slow = TcpStream::connect(env.run.http).await.unwrap();
    slow.write_all(b"GET /met").await.unwrap();
    let started = Instant::now();
    assert_eq!(get(&env, "/healthz").await.status, 200, "молчащий клиент не должен мешать другим");
    assert!(started.elapsed() < Duration::from_secs(1));
    let mut raw = Vec::new();
    let _ = within("закрытие молчащего клиента", slow.read_to_end(&mut raw)).await;
    assert!(started.elapsed() < Duration::from_secs(6), "незаконченную голову сервер обязан обрывать по сроку");
    env.run.shutdown().await;
}

#[tokio::test]
async fn admin_api_needs_a_token_over_tls() {
    let env = boot().await;
    udp(&env, "a.hits:5|c\na.time:40|ms\na.time:60|ms\nb.size:9|g").await;
    eventually(&env, "b_size 9").await;
    for token in [None, Some("wrong")] {
        let err = within("Get без права", admin(&env, token).get(GetRequest { name: "a.hits".into() })).await.unwrap_err();
        assert_eq!(err.code(), Code::Unauthenticated, "токен {token:?}");
    }
    let mut client = admin(&env, Some(TOKEN));
    let hits = within("Get", client.get(GetRequest { name: "a.hits".into() })).await.unwrap().into_inner();
    assert_eq!((hits.kind, hits.value, hits.count), (Kind::Counter as i32, 5.0, 0));
    let timer = within("Get", client.get(GetRequest { name: "a.time".into() })).await.unwrap().into_inner();
    assert_eq!((timer.kind, timer.value, timer.count), (Kind::Timer as i32, 100.0, 2), "у таймера value это сумма, count число замеров");
    assert_eq!(client.get(GetRequest { name: "zzz".into() }).await.unwrap_err().code(), Code::NotFound);
    assert_eq!(client.get(GetRequest { name: "bad name!".into() }).await.unwrap_err().code(), Code::InvalidArgument);
    let mut stream = within("List", client.list(ListRequest { prefix: "a.".into() })).await.unwrap().into_inner();
    let mut names = Vec::new();
    while let Some(m) = within("поток List", stream.message()).await.unwrap() {
        names.push(m.name);
    }
    assert_eq!(names, vec!["a.hits", "a.time"], "List отдаёт метрики с префиксом по алфавиту");
    within("Reset", client.reset(ResetRequest { name: "a.hits".into() })).await.unwrap();
    assert_eq!(client.get(GetRequest { name: "a.hits".into() }).await.unwrap_err().code(), Code::NotFound, "после Reset метрики нет");
    assert_eq!(client.reset(ResetRequest { name: "a.hits".into() }).await.unwrap_err().code(), Code::NotFound);
    assert!(!get(&env, "/metrics").await.body.contains("a_hits"), "и в /metrics её больше нет");
    env.run.shutdown().await;
}

#[tokio::test]
async fn health_is_open_and_the_admin_port_really_speaks_tls() {
    let env = boot().await;
    let tls = ClientTlsConfig::new().ca_certificate(Certificate::from_pem(&env.ca_pem)).domain_name("localhost");
    let channel = Endpoint::from_shared(format!("https://{}", env.run.grpc)).unwrap().tls_config(tls).unwrap().connect_lazy();
    let mut health = HealthClient::new(channel);
    let status = within("health", health.check(HealthCheckRequest { service: "metrics.v1.Admin".into() })).await.expect("здоровье открыто без токена").into_inner().status();
    assert_eq!(status, tonic_health::pb::health_check_response::ServingStatus::Serving);
    let plain = Endpoint::from_shared(format!("http://{}", env.run.grpc)).unwrap().timeout(Duration::from_secs(2)).connect_lazy();
    assert!(AdminClient::new(plain).get(GetRequest { name: "x".into() }).await.is_err(), "порт администрирования не должен отвечать без TLS");
    env.run.shutdown().await;
}

#[tokio::test]
async fn shutdown_lets_requests_finish_and_closes_everything() {
    let env = boot().await;
    udp(&env, "z:1|c").await;
    eventually(&env, "z_total 1").await;
    let (http, grpc, udp_addr) = (env.run.http, env.run.grpc, env.run.udp);
    let mut slow = TcpStream::connect(http).await.unwrap();
    slow.write_all(b"GET /metrics HTTP/1.1\r\n").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let started = Instant::now();
    let stopper = tokio::spawn(env.run.shutdown());
    tokio::time::sleep(Duration::from_millis(250)).await;
    slow.write_all(b"Host: x\r\n\r\n").await.unwrap();
    let reply = read_reply(&mut slow).await;
    assert_eq!(reply.status, 200, "запрос, начатый до остановки, должен быть дообслужен");
    assert!(reply.body.contains("z_total 1"));
    within("остановка", stopper).await.unwrap();
    assert!(started.elapsed() < Duration::from_secs(4), "остановка затянулась: {:?}", started.elapsed());
    assert!(TcpStream::connect(http).await.is_err(), "HTTP-порт должен быть закрыт");
    assert!(TcpStream::connect(grpc).await.is_err(), "gRPC-порт должен быть закрыт");
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let _ = socket.send_to(b"late:1|c", udp_addr).await;
}
