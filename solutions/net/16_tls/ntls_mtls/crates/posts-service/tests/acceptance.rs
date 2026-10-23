use posts_service::serve;
use proto::posts::v1::posts_client::PostsClient;
use proto::posts::v1::GetPostRequest;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tokio::net::TcpListener;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};
use tonic::Code;

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

struct Env {
    dir: PathBuf,
    addr: SocketAddr,
}

async fn env(name: &str) -> Env {
    let dir = std::env::temp_dir().join(format!("mtls-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    certgen::issue_all(&dir).expect("выпуск сертификатов");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let certs = dir.clone();
    tokio::spawn(async move {
        let _ = serve(listener, &certs).await;
    });
    Env { dir, addr }
}

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).unwrap()
}

fn channel(env: &Env, ca_dir: &Path, who: Option<&str>, domain: &str) -> Channel {
    let mut tls = ClientTlsConfig::new().ca_certificate(Certificate::from_pem(read(ca_dir, "ca.pem"))).domain_name(domain);
    if let Some(who) = who {
        tls = tls.identity(Identity::from_pem(read(&env.dir, &format!("{who}.pem")), read(&env.dir, &format!("{who}.key"))));
    }
    Endpoint::from_shared(format!("https://{}", env.addr)).unwrap().tls_config(tls).unwrap().connect_lazy()
}

async fn get(channel: Channel) -> Result<String, tonic::Status> {
    let mut client = PostsClient::new(channel);
    Ok(within("вызов", client.get_post(GetPostRequest { id: 3 })).await?.into_inner().text)
}

#[tokio::test]
async fn gateway_reads_posts() {
    let e = env("gateway").await;
    assert_eq!(get(channel(&e, &e.dir, Some("gateway"), "posts.social.test")).await.expect("шлюз"), "пост 3");
}

#[tokio::test]
async fn other_service_with_a_valid_certificate_is_forbidden() {
    let e = env("users").await;
    let err = get(channel(&e, &e.dir, Some("users"), "posts.social.test")).await.expect_err("сервису пользователей вызывать посты нельзя");
    assert_eq!(err.code(), Code::PermissionDenied, "рукопожатие прошло (CA один), но права проверяются по имени: {err}");
    let err = get(channel(&e, &e.dir, Some("posts"), "posts.social.test")).await.expect_err("и самому сервису постов нельзя");
    assert_eq!(err.code(), Code::PermissionDenied);
}

#[tokio::test]
async fn client_without_a_certificate_cannot_connect() {
    let e = env("anon").await;
    let err = get(channel(&e, &e.dir, None, "posts.social.test")).await.expect_err("сервер обязан требовать клиентский сертификат");
    assert!(matches!(err.code(), Code::Unavailable | Code::Unknown), "ждали отказ транспорта, получили {err}");
}

#[tokio::test]
async fn certificate_from_another_ca_cannot_connect() {
    let e = env("mine").await;
    let other = env("other").await;
    let mut tls = ClientTlsConfig::new().ca_certificate(Certificate::from_pem(read(&e.dir, "ca.pem"))).domain_name("posts.social.test");
    tls = tls.identity(Identity::from_pem(read(&other.dir, "gateway.pem"), read(&other.dir, "gateway.key")));
    let ch = Endpoint::from_shared(format!("https://{}", e.addr)).unwrap().tls_config(tls).unwrap().connect_lazy();
    let err = get(ch).await.expect_err("шлюз с сертификатом от чужого CA");
    assert!(matches!(err.code(), Code::Unavailable | Code::Unknown), "ждали отказ транспорта, получили {err}");
}

#[tokio::test]
async fn server_name_is_verified_by_the_client() {
    let e = env("name").await;
    let err = get(channel(&e, &e.dir, Some("gateway"), "users.social.test")).await.expect_err("сертификат сервера выдан на posts.social.test, а не на users.social.test");
    assert!(matches!(err.code(), Code::Unavailable | Code::Unknown), "ждали отказ транспорта, получили {err}");
}

#[tokio::test]
async fn gitignore_keeps_certificates_out_of_the_repository() {
    let root = std::env::temp_dir().join(format!("mtls-ignore-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    certgen::ensure_gitignore(&root).unwrap();
    assert!(std::fs::read_to_string(root.join(".gitignore")).unwrap().lines().any(|l| l == "certs/"));
}
