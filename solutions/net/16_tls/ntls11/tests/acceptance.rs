use ntls11::{identity_of, serve, Rules};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::server::WebPkiClientVerifier;
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsConnector;

#[path = "../src/pki.rs"]
mod pki;
use pki::{Ca, Leaf, Usage};

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

fn certs_of(pem: &str) -> Vec<CertificateDer<'static>> {
    CertificateDer::pem_slice_iter(pem.as_bytes()).collect::<Result<_, _>>().unwrap()
}

fn roots_of(pem: &str) -> RootCertStore {
    let mut roots = RootCertStore::empty();
    for c in certs_of(pem) {
        roots.add(c).unwrap();
    }
    roots
}

fn rules() -> Arc<Rules> {
    Arc::new(Rules::new(&[("posts.Create", &["gateway.social.test"]), ("users.Get", &["gateway.social.test", "posts.social.test"])]))
}

struct World {
    ca: Ca,
    addr: SocketAddr,
}

async fn world() -> World {
    let ca = Ca::new("CA сервисов");
    let server = ca.issue(&["localhost"], Usage::Server);
    let verifier = WebPkiClientVerifier::builder(Arc::new(roots_of(&ca.cert_pem()))).build().unwrap();
    let key = PrivateKeyDer::from_pem_slice(server.key_pem.as_bytes()).unwrap();
    let config = Arc::new(ServerConfig::builder().with_client_cert_verifier(verifier).with_single_cert(certs_of(&server.cert_pem), key).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(serve(listener, config, rules()));
    World { ca, addr }
}

async fn call(w: &World, who: &Leaf, request: &str) -> String {
    let key = PrivateKeyDer::from_pem_slice(who.key_pem.as_bytes()).unwrap();
    let config = Arc::new(ClientConfig::builder().with_root_certificates(roots_of(&w.ca.cert_pem())).with_client_auth_cert(certs_of(&who.cert_pem), key).unwrap());
    let tcp = TcpStream::connect(w.addr).await.unwrap();
    let mut tls = within("рукопожатие", TlsConnector::from(config).connect(ServerName::try_from("localhost").unwrap(), tcp)).await.unwrap();
    tls.write_all(format!("{request}\n").as_bytes()).await.unwrap();
    let mut reply = String::new();
    within("ответ", tls.read_to_string(&mut reply)).await.unwrap();
    reply
}

#[tokio::test]
async fn gateway_may_call_what_gateway_may_call() {
    let w = world().await;
    let gateway = w.ca.issue(&["gateway.social.test"], Usage::Client);
    assert_eq!(call(&w, &gateway, "CALL posts.Create").await, "OK gateway.social.test\n");
    assert_eq!(call(&w, &gateway, "CALL users.Get").await, "OK gateway.social.test\n");
}

#[tokio::test]
async fn other_service_gets_only_its_own_rights() {
    let w = world().await;
    let users = w.ca.issue(&["users.social.test"], Usage::Client);
    assert_eq!(call(&w, &users, "CALL posts.Create").await, "DENIED users.social.test\n", "сервису пользователей создание постов не разрешено");
    let posts = w.ca.issue(&["posts.social.test"], Usage::Client);
    assert_eq!(call(&w, &posts, "CALL users.Get").await, "OK posts.social.test\n");
    assert_eq!(call(&w, &posts, "CALL posts.Create").await, "DENIED posts.social.test\n");
}

#[tokio::test]
async fn unknown_method_is_denied_by_default() {
    let w = world().await;
    let gateway = w.ca.issue(&["gateway.social.test"], Usage::Client);
    assert_eq!(call(&w, &gateway, "CALL admin.DropEverything").await, "DENIED gateway.social.test\n", "запрещено всё, что не разрешено явно");
    assert_eq!(call(&w, &gateway, "HELLO posts.Create").await, "DENIED gateway.social.test\n", "строка должна начинаться с CALL");
}

#[tokio::test]
async fn claims_in_the_request_are_worthless() {
    let w = world().await;
    let users = w.ca.issue(&["users.social.test"], Usage::Client);
    assert_eq!(call(&w, &users, "CALL posts.Create AS gateway.social.test").await, "DENIED users.social.test\n", "личность берётся из сертификата, а не из слов клиента");
    assert_eq!(call(&w, &users, "CALL posts.Create gateway.social.test").await, "DENIED users.social.test\n");
}

#[tokio::test]
async fn common_name_is_not_an_identity() {
    let w = world().await;
    let sneaky = w.ca.issue_cn("gateway.social.test", &["evil.test"], Usage::Client);
    assert_eq!(call(&w, &sneaky, "CALL posts.Create").await, "DENIED evil.test\n", "CN нельзя считать личностью: имена живут в SAN");
}

#[test]
fn identity_comes_from_the_first_dns_san() {
    let ca = Ca::new("CA");
    let both = ca.issue(&["10.0.0.7", "first.test", "second.test"], Usage::Client);
    assert_eq!(identity_of(certs_of(&both.cert_pem)[0].as_ref()), Some("first.test".to_string()));
    let only_ip = ca.issue(&["10.0.0.7"], Usage::Client);
    assert_eq!(identity_of(certs_of(&only_ip.cert_pem)[0].as_ref()), None, "в SAN только IP: личности нет");
    assert_eq!(identity_of(b"not a certificate"), None);
    assert_eq!(identity_of(&[]), None);
}

#[test]
fn rules_deny_by_default() {
    let r = Rules::new(&[("a.B", &["x.test"])]);
    assert!(r.allows(Some("x.test"), "a.B"));
    assert!(!r.allows(Some("y.test"), "a.B"));
    assert!(!r.allows(Some("x.test"), "a.C"));
    assert!(!r.allows(None, "a.B"), "без личности нельзя ничего");
    assert!(!Rules::new(&[]).allows(Some("x.test"), "a.B"));
    assert!(!Rules::new(&[("a.B", &[])]).allows(Some(""), "a.B"));
}
