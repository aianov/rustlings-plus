use ntn_metadata::pb::users_client::UsersClient;
use ntn_metadata::pb::users_server::UsersServer;
use ntn_metadata::pb::GetUserRequest;
use ntn_metadata::{call_with, start, UsersSvc};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tokio_stream::StreamExt;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint, Server};
use tonic::{Code, Request};

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

async fn client() -> UsersClient<Channel> {
    let addr = start().await.unwrap();
    UsersClient::connect(format!("http://{addr}")).await.unwrap()
}

#[tokio::test]
async fn request_id_comes_back_unchanged() {
    let mut client = client().await;
    let reply = within("вызов", call_with(&mut client, 3, "abc-123", None)).await.unwrap();
    assert_eq!(reply.request_id, "abc-123", "идентификатор запроса должен вернуться в метаданных ответа");
    assert_eq!(reply.user.id, 3);
    assert_eq!(reply.trace, None, "трассы не было, и в ответе её быть не должно");
}

#[tokio::test]
async fn server_generates_distinct_ids_when_header_is_missing() {
    let mut client = client().await;
    let a = within("первый", client.get_user(GetUserRequest { id: 1 })).await.unwrap();
    let b = within("второй", client.get_user(GetUserRequest { id: 1 })).await.unwrap();
    let ida = a.metadata().get("x-request-id").expect("сервер обязан вернуть x-request-id, даже если клиент его не прислал").to_str().unwrap().to_string();
    let idb = b.metadata().get("x-request-id").unwrap().to_str().unwrap().to_string();
    assert!(!ida.is_empty() && ida != idb, "выданные идентификаторы должны быть разными: {ida:?} и {idb:?}");
}

#[tokio::test]
async fn server_survives_a_header_with_non_ascii_bytes() {
    let mut client = client().await;
    let mut request = Request::new(GetUserRequest { id: 1 });
    request.metadata_mut().insert("x-request-id", MetadataValue::try_from("привет").unwrap());
    let response = within("вызов", client.get_user(request)).await.expect("нечитаемый заголовок не повод отвечать ошибкой");
    let echoed = response.metadata().get("x-request-id").expect("идентификатор должен быть выдан").to_str().unwrap().to_string();
    assert!(!echoed.is_empty() && echoed.is_ascii(), "вместо нечитаемого значения сервер выдаёт свой идентификатор: {echoed:?}");
}

#[tokio::test]
async fn binary_metadata_survives_every_byte_value() {
    let mut client = client().await;
    let trace: Vec<u8> = (0..=255u8).collect();
    let reply = within("вызов", call_with(&mut client, 1, "r1", Some(&trace))).await.unwrap();
    assert_eq!(reply.trace, Some(trace), "бинарные метаданные (ключ на -bin) дошли искажёнными");
}

#[tokio::test]
async fn bad_ids_are_rejected_on_the_client_before_any_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let counter = accepted.clone();
    let incoming = TcpListenerStream::new(listener).map(move |c| {
        if c.is_ok() {
            counter.fetch_add(1, Ordering::SeqCst);
        }
        c
    });
    tokio::spawn(Server::builder().add_service(UsersServer::new(UsersSvc::default())).serve_with_incoming(incoming));
    let channel = Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_lazy();
    let mut client = UsersClient::new(channel);
    let long = "x".repeat(65);
    for bad in ["", "привет", "line\nbreak", "tab\tsep", long.as_str()] {
        let err = within("вызов", call_with(&mut client, 1, bad, None)).await.expect_err("плохой идентификатор должен быть отвергнут");
        assert_eq!(err.code(), Code::InvalidArgument, "идентификатор {bad:?} недопустим");
    }
    assert_eq!(accepted.load(Ordering::SeqCst), 0, "плохие идентификаторы должны отсеиваться до отправки, а не на сервере");
    within("вызов", call_with(&mut client, 1, &"x".repeat(64), None)).await.expect("64 символа допустимо");
}
