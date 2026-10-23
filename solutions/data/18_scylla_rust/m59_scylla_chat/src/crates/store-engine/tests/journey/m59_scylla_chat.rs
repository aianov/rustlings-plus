use store_engine::{
    Cursor, StoreConfig, StoreError, apply, connect, decode_state, drop_keyspace, encode_state,
    good_keyspace, micros_of, statements, timeuuid,
};

#[test]
fn cursors_are_opaque_and_checked() {
    let cursor = Cursor {
        bucket: 24_321,
        state: Some(bytes_of(&[0, 15, 255])),
    };
    let raw = cursor.encode();
    assert_eq!(raw, "24321.000fff");
    assert_eq!(Cursor::decode(&raw), Ok(cursor));
    assert_eq!(Cursor::decode("7.").map(|cursor| cursor.state), Ok(None));
    for broken in ["", "x.00", "1.0", "1.zz", "12"] {
        assert_eq!(
            Cursor::decode(broken),
            Err(StoreError::BadCursor),
            "{broken}"
        );
    }
    assert_eq!(
        decode_state(&encode_state(b"abc")).as_deref(),
        Some(&b"abc"[..])
    );
}

fn bytes_of(raw: &[u8]) -> bytes::Bytes {
    decode_state(&encode_state(raw)).expect("байты")
}

#[test]
fn timeuuids_keep_time_order() {
    let early = timeuuid(1_700_000_000_000_000, 1);
    let late = timeuuid(1_700_000_000_000_500, 2);
    assert_eq!(early.get_version_num(), 1);
    assert_eq!(micros_of(&early), Some(1_700_000_000_000_000));
    assert!(micros_of(&late) > micros_of(&early));
    assert!(good_keyspace("chat_42") && !good_keyspace("Chat") && !good_keyspace("1x"));
    assert_eq!(
        statements("CREATE TABLE a (x int);\n-- комментарий\nCREATE TABLE b (y int);\n"),
        ["CREATE TABLE a (x int)", "CREATE TABLE b (y int)"]
    );
}

#[tokio::test]
async fn keyspace_and_schema_are_idempotent() {
    let config = StoreConfig::from_env(&format!("t_store_{}", std::process::id()));
    let _ = drop_keyspace(&config).await;
    let session = connect(&config)
        .await
        .expect("Scylla курса на 127.0.0.1:59042");
    let schema = [(
        "0001_pets.cql",
        "CREATE TABLE IF NOT EXISTS pets (id int PRIMARY KEY, name text);",
    )];
    assert_eq!(apply(&session, &schema).await.expect("схема"), 1);
    assert_eq!(apply(&session, &schema).await.expect("повтор"), 1);
    session
        .query("INSERT INTO pets (id, name) VALUES (1, 'rex')", ())
        .await
        .expect("строка");
    let broken = apply(&session, &[("0002_bad.cql", "CREATE TABLE (;")]).await;
    assert!(matches!(broken, Err(StoreError::Schema(ref why)) if why.starts_with("0002_bad.cql")));
    drop_keyspace(&config).await.expect("keyspace убран");
}
