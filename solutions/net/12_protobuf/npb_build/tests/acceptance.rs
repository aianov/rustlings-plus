use prost::Message;
use prost_types::Timestamp;
use social_proto::social::v1::{Author, Post, Visibility};

fn sample() -> Post {
    Post {
        id: 1,
        author: Some(Author { id: 2, handle: "ann".to_string() }),
        text: "hi".to_string(),
        created_at: Some(Timestamp { seconds: 10, nanos: 5 }),
        hashtags: vec!["a".to_string(), "bc".to_string()],
        visibility: Visibility::Followers as i32,
    }
}

const BYTES: [u8; 30] = [
    0x08, 0x01, 0x12, 0x07, 0x08, 0x02, 0x12, 0x03, b'a', b'n', b'n', 0x1a, 0x02, b'h', b'i', 0x22, 0x04, 0x08, 0x0a, 0x10, 0x05, 0x2a, 0x01, b'a', 0x2a, 0x02, b'b', b'c', 0x30, 0x02,
];

#[test]
fn post_encodes_to_the_agreed_bytes() {
    assert_eq!(sample().encode_to_vec(), BYTES.to_vec(), "номера и типы полей должны совпадать с таблицей из README");
}

#[test]
fn post_decodes_from_the_agreed_bytes() {
    assert_eq!(Post::decode(&BYTES[..]).unwrap(), sample());
}

#[test]
fn timestamp_is_the_well_known_type_not_a_copy() {
    let post: Post = sample();
    let stamp: Option<prost_types::Timestamp> = post.created_at;
    assert_eq!(stamp.map(|t| t.seconds), Some(10), "created_at должно быть prost_types::Timestamp, а не сгенерированной копией");
}

#[test]
fn enum_has_clean_rust_names() {
    assert_eq!(Visibility::try_from(0).unwrap(), Visibility::Unspecified);
    assert_eq!(Visibility::try_from(1).unwrap(), Visibility::Public);
    assert_eq!(Visibility::try_from(2).unwrap(), Visibility::Followers);
    assert!(Visibility::try_from(3).is_err(), "неизвестное значение перечисления не должно превращаться в вариант");
    assert_eq!(Visibility::Followers.as_str_name(), "VISIBILITY_FOLLOWERS", "в схеме имена значений с префиксом перечисления");
}

#[test]
fn defaults_stay_off_the_wire() {
    let post = Post::default();
    assert!(post.encode_to_vec().is_empty());
    assert!(post.author.is_none(), "вложенные сообщения в proto3 всегда Option");
}

fn read(path: &str) -> String {
    let full = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), path);
    std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("не нашёл {path}: {e}"))
}

#[test]
fn proto_files_are_organized_by_package_and_version() {
    let common = read("proto/social/v1/common.proto");
    let post = read("proto/social/v1/post.proto");
    assert!(common.contains("syntax = \"proto3\""));
    assert!(common.contains("package social.v1;"), "package должен повторять путь папок: social.v1");
    assert!(post.contains("package social.v1;"));
    assert!(post.contains("import \"social/v1/common.proto\";"), "импорт идёт от корня include-пути (proto/), а не относительно файла");
    assert!(post.contains("import \"google/protobuf/timestamp.proto\";"));
}

#[test]
fn build_script_reruns_when_protos_change() {
    let build = read("build.rs");
    assert!(build.contains("cargo:rerun-if-changed"), "без rerun-if-changed cargo пересоберёт крейт на любое изменение, а иногда не пересоберёт вовсе");
    assert!(build.contains("compile_protos"));
}
