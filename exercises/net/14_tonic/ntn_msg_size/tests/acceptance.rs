use ntn_msg_size::pb::files_client::FilesClient;
use ntn_msg_size::pb::{Blob, DownloadRequest};
use ntn_msg_size::{client, start};
use tonic::transport::{Channel, Endpoint};

const MIB: usize = 1024 * 1024;

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

async fn channel() -> Channel {
    let addr = start().await.unwrap();
    Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_lazy()
}

#[tokio::test]
async fn six_mib_download_works() {
    let mut c = client(channel().await);
    let blob = within("скачивание", c.download(DownloadRequest { size: (6 * MIB) as u64 })).await.expect("6 МиБ должны пройти: лимит клиента на входящие не поднят?");
    assert_eq!(blob.into_inner().data.len(), 6 * MIB);
}

#[tokio::test]
async fn six_mib_upload_works() {
    let mut c = client(channel().await);
    let reply = within("загрузка", c.upload(Blob { data: vec![1; 6 * MIB] })).await.expect("6 МиБ должны пройти: лимит сервера на входящие не поднят?");
    assert_eq!(reply.into_inner().size, (6 * MIB) as u64);
}

#[tokio::test]
async fn nine_mib_is_refused_both_ways() {
    let mut c = client(channel().await);
    assert!(within("скачивание", c.download(DownloadRequest { size: (9 * MIB) as u64 })).await.is_err(), "9 МиБ превышают лимит 8 МиБ, а вызов прошёл: лимит снят совсем?");
    assert!(within("загрузка", c.upload(Blob { data: vec![1; 9 * MIB] })).await.is_err(), "9 МиБ превышают лимит 8 МиБ, а вызов прошёл: лимит снят совсем?");
}

#[tokio::test]
async fn server_protects_itself_from_an_unlimited_client() {
    let ch = channel().await;
    let mut greedy = FilesClient::new(ch).max_encoding_message_size(usize::MAX).max_decoding_message_size(usize::MAX);
    let up = within("загрузка", greedy.upload(Blob { data: vec![1; 9 * MIB] })).await;
    assert!(up.is_err(), "сервер принял 9 МиБ от клиента без лимитов: у сервера свой лимит на входящие");
    let down = within("скачивание", greedy.download(DownloadRequest { size: (9 * MIB) as u64 })).await;
    assert!(down.is_err(), "сервер отдал 9 МиБ клиенту без лимитов: у сервера свой лимит на исходящие");
}
