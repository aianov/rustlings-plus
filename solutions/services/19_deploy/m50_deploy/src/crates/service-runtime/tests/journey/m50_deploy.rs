use std::path::PathBuf;

use service_runtime::Source;

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("m50")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("папка");
    dir
}

#[test]
fn secrets_come_from_files() {
    let dir = scratch("secrets");
    let key = dir.join("link_key");
    std::fs::write(&key, "очень-длинный-ключ\n").expect("файл");
    let source = Source::from_pairs([("GATEWAY_LINK_KEY_FILE", key.display().to_string())])
        .resolve_files()
        .expect("секрет читается");
    assert_eq!(
        source.get("GATEWAY_LINK_KEY"),
        Some("очень-длинный-ключ"),
        "значение из файла без перевода строки в конце"
    );
    let both = Source::from_pairs([
        ("GATEWAY_LINK_KEY", "прямо".to_string()),
        ("GATEWAY_LINK_KEY_FILE", key.display().to_string()),
    ])
    .resolve_files();
    assert!(
        both.is_err(),
        "и значение, и файл сразу это ошибка конфигурации"
    );
    let missing = Source::from_pairs([("AUTH_SECRET_FILE", dir.join("нет").display().to_string())])
        .resolve_files()
        .expect_err("файла нет");
    assert!(missing.to_string().contains("нет"), "{missing}");
}
