use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;

use grpc_kit::{Gate, Tls, channel};
use service_runtime::CancellationToken;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::Code;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::{Certificate, ClientTlsConfig, Endpoint, Server};
use tonic_health::ServingStatus;
use tonic_health::pb::HealthCheckRequest;
use tonic_health::pb::health_client::HealthClient;
use tonic_health::server::health_reporter;

const PROBE: &str = "probe.v1.Probe";

fn certs(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("m32-kit")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    certgen::issue_all(&dir, false).expect("сертификаты выпускаются");
    dir
}

async fn guarded(tls: &Tls, gate: Gate, stop: &CancellationToken) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = listener.local_addr().expect("адрес");
    let (reporter, health) = health_reporter();
    reporter
        .set_service_status(PROBE, ServingStatus::Serving)
        .await;
    let server = Server::builder()
        .tls_config(tls.server())
        .expect("TLS сервера настраивается");
    let stop = stop.clone();
    tokio::spawn(async move {
        let mut server = server;
        server
            .add_service(InterceptedService::new(health, gate))
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
                stop.cancelled().await
            })
            .await
    });
    addr
}

async fn probe(channel: tonic::transport::Channel) -> Result<(), tonic::Status> {
    let ask = HealthCheckRequest {
        service: PROBE.to_string(),
    };
    HealthClient::new(channel).check(ask).await.map(drop)
}

#[tokio::test]
async fn mutual_tls_needs_a_client_certificate_from_our_ca() {
    let dir = certs("mutual");
    let user = Tls::load(&dir, "user-service").expect("сертификаты user-service");
    let gateway = Tls::load(&dir, "api-gateway").expect("сертификаты шлюза");
    let stop = CancellationToken::new();
    let addr = guarded(&user, Gate::open(), &stop).await.to_string();
    let with_identity = channel(&addr, "user-service", Some(&gateway)).expect("канал");
    probe(with_identity)
        .await
        .expect("клиент со своим сертификатом проходит");
    let anonymous = Endpoint::from_shared(format!("https://{addr}"))
        .expect("адрес")
        .tls_config(
            ClientTlsConfig::new()
                .ca_certificate(Certificate::from_pem(&user.ca))
                .domain_name("user-service"),
        )
        .expect("TLS клиента")
        .connect_lazy();
    assert!(
        probe(anonymous).await.is_err(),
        "без клиентского сертификата сервер рвёт рукопожатие: это и есть взаимный TLS"
    );
    let plain = channel(&addr, "user-service", None).expect("канал");
    assert!(
        probe(plain).await.is_err(),
        "открытым текстом сервер не говорит"
    );
    let other = Tls::load(&certs("other"), "api-gateway").expect("чужие сертификаты");
    let stranger = channel(&addr, "user-service", Some(&other)).expect("канал");
    assert!(
        probe(stranger).await.is_err(),
        "сертификат от чужого CA не проходит, даже если имя то же"
    );
    let wrong_name = channel(&addr, "post-service", Some(&gateway)).expect("канал");
    assert!(
        probe(wrong_name).await.is_err(),
        "клиент проверяет имя сервера: у user-service нет post-service в SAN"
    );
    stop.cancel();
}

#[tokio::test]
async fn gate_lets_in_only_named_peers() {
    let dir = certs("gate");
    let post = Tls::load(&dir, "post-service").expect("сертификаты post-service");
    let stop = CancellationToken::new();
    let addr = guarded(&post, Gate::only(&["api-gateway"]), &stop)
        .await
        .to_string();
    let gateway = Tls::load(&dir, "api-gateway").expect("сертификаты шлюза");
    probe(channel(&addr, "post-service", Some(&gateway)).expect("канал"))
        .await
        .expect("шлюз проходит по SAN api-gateway");
    let chat = Tls::load(&dir, "chat-service").expect("сертификаты chat-service");
    let refused = probe(channel(&addr, "post-service", Some(&chat)).expect("канал"))
        .await
        .expect_err("chat-service не шлюз");
    assert_eq!(
        refused.code(),
        Code::PermissionDenied,
        "сертификат настоящий, но имя в SAN не то: {refused:?}"
    );
    assert!(refused.message().contains("chat-service"), "{refused:?}");
    stop.cancel();
}

#[test]
fn missing_certificates_point_to_xtask() {
    let empty = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("m32-kit-empty");
    let _ = fs::remove_dir_all(&empty);
    let error = Tls::load(&empty, "user-service").expect_err("сертификатов нет");
    assert!(
        error.to_string().contains("cargo xtask certs"),
        "ошибка подсказывает, чем выпустить сертификаты: {error}"
    );
}
