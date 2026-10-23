use std::path::PathBuf;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("certgen-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn all_files_are_written_and_keys_are_private() {
    let dir = temp("files").join("certs");
    certgen::issue_all(&dir).expect("выпуск сертификатов");
    let mut names: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    names.sort();
    assert_eq!(names, vec!["ca.key", "ca.pem", "gateway.key", "gateway.pem", "posts.key", "posts.pem", "users.key", "users.pem"]);
    for name in &names {
        let text = std::fs::read_to_string(dir.join(name)).unwrap();
        if name.ends_with(".key") {
            assert!(text.contains("PRIVATE KEY"), "{name}: ждали закрытый ключ");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(dir.join(name)).unwrap().permissions().mode() & 0o777;
                assert_eq!(mode & 0o077, 0, "{name}: ключ доступен другим пользователям (права {mode:o})");
            }
        } else {
            assert!(text.starts_with("-----BEGIN CERTIFICATE-----"), "{name}: ждали сертификат");
        }
    }
    let _ = std::fs::remove_dir_all(dir.parent().unwrap());
}

#[test]
fn gitignore_is_created_extended_and_never_duplicated() {
    let root = temp("gitignore");
    certgen::ensure_gitignore(&root).unwrap();
    assert_eq!(std::fs::read_to_string(root.join(".gitignore")).unwrap(), "certs/\n", "файла не было: он создаётся");
    certgen::ensure_gitignore(&root).unwrap();
    assert_eq!(std::fs::read_to_string(root.join(".gitignore")).unwrap(), "certs/\n", "повторный вызов не должен дублировать строку");

    std::fs::write(root.join(".gitignore"), "target/\n.env").unwrap();
    certgen::ensure_gitignore(&root).unwrap();
    assert_eq!(std::fs::read_to_string(root.join(".gitignore")).unwrap(), "target/\n.env\ncerts/\n", "чужие строки сохраняются, а строка добавляется с новой строки");

    std::fs::write(root.join(".gitignore"), "target/\ncerts/\n.env\n").unwrap();
    certgen::ensure_gitignore(&root).unwrap();
    assert_eq!(std::fs::read_to_string(root.join(".gitignore")).unwrap(), "target/\ncerts/\n.env\n", "строка уже есть: файл не меняется");
    let _ = std::fs::remove_dir_all(root);
}
