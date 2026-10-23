use proto::users::v1::users_client::UsersClient;
use proto::users::v1::GetUserRequest;
use std::time::Duration;

#[tokio::test]
async fn users_service_answers_over_the_network() {
    let addr = users_service::start().await.unwrap();
    let mut client = UsersClient::connect(format!("http://{addr}")).await.unwrap();
    let user = tokio::time::timeout(Duration::from_secs(5), client.get_user(GetUserRequest { id: 1 })).await.expect("вызов завис").unwrap().into_inner();
    assert_eq!(user, users_service::sample_user());
}

#[test]
fn service_does_not_build_schemas_itself() {
    let root = env!("CARGO_MANIFEST_DIR");
    assert!(!std::path::Path::new(&format!("{root}/build.rs")).exists(), "у сервиса не должно быть своего build.rs: схемы собирает крейт proto");
    let manifest = std::fs::read_to_string(format!("{root}/Cargo.toml")).unwrap();
    assert!(!manifest.contains("tonic-prost-build"), "сервису не нужна tonic-prost-build: схемы собирает крейт proto");
    assert!(manifest.contains("proto") && manifest.contains("path"), "сервис должен зависеть от крейта proto через path");
}
