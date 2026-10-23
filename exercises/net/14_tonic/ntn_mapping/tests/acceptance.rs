use ntn_mapping::pb::posts_client::PostsClient;
use ntn_mapping::pb::{CreatePostRequest, Visibility as PbVis};
use ntn_mapping::{post_to_proto, start, AuthorId, NewPost, Visibility};
use std::num::NonZeroU64;
use tonic::{Code, Status};

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

fn req(author_id: u64, text: &str, visibility: i32) -> CreatePostRequest {
    CreatePostRequest { author_id, text: text.to_string(), visibility }
}

fn rejected(r: CreatePostRequest, field: &str) {
    let err: Status = NewPost::try_from(r).expect_err("плохой ввод должен быть отвергнут");
    assert_eq!(err.code(), Code::InvalidArgument, "для плохого поля {field} нужен InvalidArgument");
    assert!(err.message().contains(field), "в сообщении должно быть имя поля {field}: {}", err.message());
}

#[test]
fn good_request_becomes_a_domain_value() {
    let post = NewPost::try_from(req(5, "  привет, мир  ", PbVis::Followers as i32)).unwrap();
    assert_eq!(post, NewPost { author: AuthorId(NonZeroU64::new(5).unwrap()), text: "привет, мир".to_string(), visibility: Visibility::Followers });
}

#[test]
fn zero_author_is_rejected() {
    rejected(req(0, "hi", PbVis::Public as i32), "author_id");
}

#[test]
fn blank_text_is_rejected() {
    rejected(req(1, "", PbVis::Public as i32), "text");
    rejected(req(1, "   \n\t ", PbVis::Public as i32), "text");
}

#[test]
fn length_limit_counts_characters_not_bytes() {
    let ok = "ж".repeat(280);
    assert!(NewPost::try_from(req(1, &ok, PbVis::Public as i32)).is_ok(), "280 русских букв это 560 байт, но всего 280 символов: пост допустим");
    rejected(req(1, &"ж".repeat(281), PbVis::Public as i32), "text");
    rejected(req(1, &format!("{}{}", " ".repeat(10), "x".repeat(281)), PbVis::Public as i32), "text");
}

#[test]
fn unspecified_and_unknown_visibility_are_rejected() {
    rejected(req(1, "hi", PbVis::Unspecified as i32), "visibility");
    rejected(req(1, "hi", 9), "visibility");
    rejected(req(1, "hi", -1), "visibility");
}

#[test]
fn back_to_proto_keeps_every_field() {
    let new = NewPost::try_from(req(5, "hi", PbVis::Public as i32)).unwrap();
    let post = post_to_proto(77, &new);
    assert_eq!((post.id, post.author_id, post.text.as_str(), post.visibility), (77, 5, "hi", PbVis::Public as i32));
}

#[tokio::test]
async fn whole_path_over_the_network() {
    let addr = start().await.unwrap();
    let mut client = PostsClient::connect(format!("http://{addr}")).await.unwrap();
    let post = within("create", client.create_post(req(2, "ok", PbVis::Public as i32))).await.unwrap().into_inner();
    assert_eq!((post.id, post.author_id), (1, 2));
    let err = within("create", client.create_post(req(2, "ok", 42))).await.unwrap_err();
    assert_eq!(err.code(), Code::InvalidArgument, "клиент должен увидеть InvalidArgument, а не Internal и не пустую запись");
}
