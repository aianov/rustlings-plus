use ntx_client_stream::pb::like_request::Kind;
use ntx_client_stream::pb::likes_client::LikesClient;
use ntx_client_stream::pb::{Like, LikeRequest};
use ntx_client_stream::{send_likes, start, LikeStore};
use std::time::Duration;
use tokio_stream::StreamExt;
use tonic::transport::{Channel, Endpoint};
use tonic::Code;

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

async fn setup() -> (LikesClient<Channel>, std::sync::Arc<LikeStore>) {
    let store = LikeStore::new();
    let addr = start(store.clone()).await.unwrap();
    (LikesClient::new(Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_lazy()), store)
}

fn like(post_id: u64) -> LikeRequest {
    LikeRequest { kind: Some(Kind::Like(Like { post_id })) }
}

#[tokio::test]
async fn summary_counts_likes_and_distinct_posts() {
    let (mut client, store) = setup().await;
    let summary = within("пачка", send_likes(&mut client, vec![5, 5, 7, 5, 9])).await.unwrap();
    assert_eq!((summary.accepted, summary.distinct_posts), (5, 3));
    assert_eq!((store.count(5), store.count(7), store.count(9)), (3, 1, 1), "лайки должны лечь в хранилище по постам");
}

#[tokio::test]
async fn empty_batch_is_fine() {
    let (mut client, store) = setup().await;
    let summary = within("пачка", send_likes(&mut client, vec![])).await.unwrap();
    assert_eq!((summary.accepted, summary.distinct_posts, store.total()), (0, 0, 0));
}

#[tokio::test]
async fn invalid_like_in_the_middle_leaves_nothing_behind() {
    let (mut client, store) = setup().await;
    let err = within("пачка", send_likes(&mut client, vec![1, 2, 3, 0, 4])).await.unwrap_err();
    assert_eq!(err.code(), Code::InvalidArgument);
    assert_eq!(store.total(), 0, "вызов не удался, но часть лайков уже записана: применять надо после Commit, когда всё прочитано без ошибок");
}

#[tokio::test]
async fn thousand_pass_and_one_more_does_not() {
    let (mut client, store) = setup().await;
    let ok = within("пачка", send_likes(&mut client, vec![1; 1000])).await.unwrap();
    assert_eq!(ok.accepted, 1000);
    let err = within("пачка", send_likes(&mut client, vec![2; 1001])).await.unwrap_err();
    assert_eq!(err.code(), Code::ResourceExhausted, "1001 лайк в одной пачке сверх предела");
    assert_eq!(store.count(2), 0, "отклонённая пачка не должна ничего записывать");
    assert_eq!(store.count(1), 1000);
}

#[tokio::test]
async fn endless_stream_is_cut_off_instead_of_read_to_the_end() {
    let (mut client, store) = setup().await;
    let endless = tokio_stream::iter(std::iter::repeat(like(1)));
    let err = within("бесконечная пачка", client.send_batch(endless)).await.unwrap_err();
    assert_eq!(err.code(), Code::ResourceExhausted, "сервер должен остановиться на пределе, а не копить поток целиком");
    assert_eq!(store.total(), 0);
}

#[tokio::test]
async fn stream_that_ends_without_commit_is_not_applied() {
    let (mut client, store) = setup().await;
    let err = within("пачка", client.send_batch(tokio_stream::iter(vec![like(1), like(2)]))).await.unwrap_err();
    assert_eq!(err.code(), Code::Aborted, "поток закончился без Commit: клиент мог оборваться, пачка не применяется");
    assert_eq!(store.total(), 0);
}

#[tokio::test]
async fn client_that_hangs_up_mid_batch_writes_nothing() {
    let (client, store) = setup().await;
    let mut c = client.clone();
    let call = tokio::spawn(async move {
        let stalled = tokio_stream::iter(vec![like(1), like(2)]).chain(tokio_stream::pending());
        c.send_batch(stalled).await
    });
    tokio::time::sleep(Duration::from_millis(150)).await;
    call.abort();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(store.total(), 0, "клиент ушёл, не закончив пачку: для сервера это конец потока, и только отсутствие Commit отличает его от настоящего конца");
}
