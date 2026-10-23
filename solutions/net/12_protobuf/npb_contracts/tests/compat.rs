use std::collections::BTreeMap;

use prost::Message;
use proto::social::v1::{ChatMessage, Post, Role, User, post};

mod v1 {
    use prost::Message;

    #[derive(Clone, PartialEq, Message)]
    pub struct User {
        #[prost(uint64, tag = "1")]
        pub id: u64,
        #[prost(string, tag = "2")]
        pub handle: String,
        #[prost(int32, tag = "3")]
        pub followers: i32,
        #[prost(bool, tag = "4")]
        pub legacy_flag: bool,
    }

    #[derive(Clone, PartialEq, Message)]
    pub struct Post {
        #[prost(uint64, tag = "1")]
        pub id: u64,
        #[prost(uint64, tag = "2")]
        pub author_id: u64,
        #[prost(string, tag = "3")]
        pub text: String,
        #[prost(int32, tag = "4")]
        pub likes: i32,
    }

    #[derive(Clone, PartialEq, Message)]
    pub struct ChatMessage {
        #[prost(uint64, tag = "1")]
        pub id: u64,
        #[prost(uint64, tag = "2")]
        pub from: u64,
        #[prost(uint64, tag = "3")]
        pub to: u64,
        #[prost(string, tag = "4")]
        pub body: String,
    }
}

const OLD_USER: [u8; 12] = [0x08, 0x07, 0x12, 0x03, b'a', b'n', b'n', 0x18, 0xdc, 0x0b, 0x20, 0x01];
const OLD_POST: [u8; 10] = [0x08, 0x01, 0x10, 0x07, 0x1a, 0x02, b'h', b'i', 0x20, 0x03];
const OLD_CHAT: [u8; 10] = [0x08, 0x09, 0x10, 0x07, 0x18, 0x08, 0x22, 0x02, b'y', b'o'];

const NEW_USER: [u8; 16] = [0x08, 0x07, 0x12, 0x03, b'a', b'n', b'n', 0x18, 0xdc, 0x0b, 0x2a, 0x02, b'h', b'i', 0x30, 0x02];
const NEW_POST: [u8; 23] = [0x08, 0x01, 0x10, 0x07, 0x1a, 0x02, b'h', b'i', 0x20, 0x03, 0x2a, 0x01, b'a', 0x32, 0x05, 0x0a, 0x01, b'x', 0x10, 0x02, 0x42, 0x01, b'v'];
const NEW_CHAT: [u8; 14] = [0x08, 0x09, 0x10, 0x07, 0x18, 0x08, 0x22, 0x02, b'y', b'o', 0x2a, 0x02, 0x08, 0x64];

fn schema_text() -> String {
    let path = format!("{}/proto/social/v1/social.proto", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).expect("файл схемы должен лежать на месте")
}

#[test]
fn old_bytes_are_read_by_the_new_types() {
    let user = User::decode(&OLD_USER[..]).unwrap();
    assert_eq!((user.id, user.handle.as_str(), user.followers), (7, "ann", 1500), "поля, что были в первой версии, читаются как прежде");
    assert_eq!(user.bio, None);
    assert_eq!(user.role, 0);

    let post = Post::decode(&OLD_POST[..]).unwrap();
    assert_eq!((post.id, post.author_id, post.text.as_str(), post.like_count), (1, 7, "hi", 3), "счётчик лайков остался в поле 4 и просто стал шире");
    assert!(post.hashtags.is_empty() && post.reactions.is_empty() && post.media.is_none());

    let chat = ChatMessage::decode(&OLD_CHAT[..]).unwrap();
    assert_eq!((chat.id, chat.sender, chat.to, chat.body.as_str()), (9, 7, 8, "yo"), "поле from переименовано в sender, номер 2 тот же");
    assert!(chat.sent_at.is_none());
}

#[test]
fn new_messages_encode_to_the_agreed_bytes() {
    let user = User { id: 7, handle: "ann".into(), followers: 1500, bio: Some("hi".into()), role: Role::Moderator as i32 };
    assert_eq!(user.encode_to_vec(), NEW_USER.to_vec(), "номера и типы User не совпали с таблицей в README");

    let reactions: BTreeMap<String, u32> = [("x".to_string(), 2)].into_iter().collect();
    let post = Post { id: 1, author_id: 7, text: "hi".into(), like_count: 3, hashtags: vec!["a".into()], reactions, media: Some(post::Media::VideoUrl("v".into())) };
    assert_eq!(post.encode_to_vec(), NEW_POST.to_vec(), "номера и типы Post не совпали с таблицей в README");

    let chat = ChatMessage { id: 9, sender: 7, to: 8, body: "yo".into(), sent_at: Some(prost_types::Timestamp { seconds: 100, nanos: 0 }) };
    assert_eq!(chat.encode_to_vec(), NEW_CHAT.to_vec(), "номера и типы ChatMessage не совпали с таблицей в README");
}

#[test]
fn old_readers_understand_new_bytes() {
    let user = v1::User::decode(&NEW_USER[..]).unwrap();
    assert_eq!((user.id, user.handle.as_str(), user.followers, user.legacy_flag), (7, "ann", 1500, false), "читатель первой версии пропускает новые поля 5 и 6");
    let post = v1::Post::decode(&NEW_POST[..]).unwrap();
    assert_eq!((post.id, post.author_id, post.text.as_str(), post.likes), (1, 7, "hi", 3));
    let chat = v1::ChatMessage::decode(&NEW_CHAT[..]).unwrap();
    assert_eq!((chat.id, chat.from, chat.to, chat.body.as_str()), (9, 7, 8, "yo"));
}

#[test]
fn legacy_flag_is_reserved_not_recycled() {
    let text = schema_text();
    let reserved: Vec<&str> = text.lines().map(str::trim).filter(|l| l.starts_with("reserved")).collect();
    assert!(reserved.iter().any(|l| l.trim_end_matches(';').split_whitespace().last() == Some("4")), "номер 4 нужно зарезервировать: reserved 4; (найдено: {reserved:?})");
    assert!(reserved.iter().any(|l| l.contains("\"legacy_flag\"")), "имя тоже резервируется: reserved \"legacy_flag\"; (найдено: {reserved:?})");
    let user_block = text.split("message User").nth(1).and_then(|rest| rest.split('}').next()).expect("в схеме нет message User");
    assert!(!user_block.lines().any(|l| l.contains("= 4;")), "номер 4 в User занимать нельзя");
}

#[test]
fn bio_distinguishes_empty_from_absent() {
    let absent = User { id: 1, ..User::default() };
    let empty = User { id: 1, bio: Some(String::new()), ..User::default() };
    assert_eq!(absent.encode_to_vec(), vec![0x08, 0x01]);
    assert_eq!(empty.encode_to_vec(), vec![0x08, 0x01, 0x2a, 0x00], "пустая биография задана, и это не то же самое, что её отсутствие");
    assert_eq!(User::decode(&empty.encode_to_vec()[..]).unwrap().bio, Some(String::new()));
}

#[test]
fn unknown_role_from_a_newer_peer_is_kept() {
    let bytes = [0x08, 0x01, 0x30, 0x09];
    let user = User::decode(&bytes[..]).unwrap();
    assert_eq!(user.role, 9, "перечисление хранится числом, неизвестное значение не теряется");
    assert!(Role::try_from(user.role).is_err());
    assert_eq!(user.encode_to_vec(), bytes.to_vec());
}

#[test]
fn media_oneof_last_variant_wins() {
    let bytes = [0x3a, 0x01, b'i', 0x42, 0x01, b'v'];
    let post = Post::decode(&bytes[..]).unwrap();
    assert_eq!(post.media, Some(post::Media::VideoUrl("v".into())));
}

#[test]
fn reactions_encode_the_same_way_every_time() {
    let pairs = [("like", 5u32), ("wow", 1), ("haha", 2), ("sad", 3), ("fire", 4), ("clap", 6)];
    let reference = Post { reactions: pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect(), ..Post::default() }.encode_to_vec();
    for shift in 0..pairs.len() {
        let mut rotated = pairs.to_vec();
        rotated.rotate_left(shift);
        let post = Post { reactions: rotated.iter().map(|(k, v)| (k.to_string(), *v)).collect(), ..Post::default() };
        assert_eq!(post.encode_to_vec(), reference);
    }
}
