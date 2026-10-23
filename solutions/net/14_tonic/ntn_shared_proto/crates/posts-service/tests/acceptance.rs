use proto::posts::v1::posts_client::PostsClient;
use proto::posts::v1::{GetPostRequest, Post};
use std::time::Duration;

#[tokio::test]
async fn posts_service_answers_over_the_network() {
    let addr = posts_service::start().await.unwrap();
    let mut client = PostsClient::connect(format!("http://{addr}")).await.unwrap();
    let post = tokio::time::timeout(Duration::from_secs(5), client.get_post(GetPostRequest { id: 4 })).await.expect("вызов завис").unwrap().into_inner();
    assert_eq!((post.id, post.text.as_str()), (4, "пост 4"));
    assert_eq!(post.author.unwrap().handle, "ann");
}

#[test]
fn user_from_users_service_fits_into_a_post_without_conversion() {
    let author: proto::users::v1::User = users_service::sample_user();
    let post = Post { id: 1, author: Some(author), text: "hi".to_string() };
    assert_eq!(post.author.map(|a| a.id), Some(1), "User из users-service и User внутри Post должны быть одним типом из крейта proto");
}

#[test]
fn service_does_not_build_schemas_itself() {
    let root = env!("CARGO_MANIFEST_DIR");
    assert!(!std::path::Path::new(&format!("{root}/build.rs")).exists(), "у сервиса не должно быть своего build.rs: схемы собирает крейт proto");
    let manifest = std::fs::read_to_string(format!("{root}/Cargo.toml")).unwrap();
    assert!(!manifest.contains("tonic-prost-build"), "сервису не нужна tonic-prost-build: схемы собирает крейт proto");
}

#[test]
fn proto_crate_reruns_when_schemas_change() {
    let script = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../proto/build.rs")).unwrap();
    assert!(script.contains("cargo:rerun-if-changed"), "без rerun-if-changed правка схемы не пересоберёт код");
}
