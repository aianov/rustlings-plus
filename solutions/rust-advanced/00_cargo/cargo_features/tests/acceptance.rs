use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn cargo(sub: &[&str], flags: &[&str], target: &str) -> Output {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(target);
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(sub)
        .arg("--target-dir")
        .arg(dir)
        .args(flags)
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .output()
        .expect("не удалось запустить cargo")
}

fn probe(flags: &[&str], target: &str) -> String {
    let out = cargo(&["run", "--quiet", "--example", "probe"], flags, target);
    assert!(
        out.status.success(),
        "пример probe не собрался с флагами {flags:?}:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn tree_has(flags: &[&str], name: &str) -> bool {
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["tree", "--depth", "1", "--prefix", "none", "--format", "{p}"])
        .args(flags)
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.split_whitespace().next() == Some(name))
}

#[test]
fn json_is_on_by_default() {
    assert_eq!(probe(&[], "f_default"), "json", "по умолчанию должна быть включена только feature json");
}

#[test]
fn nothing_without_default_features() {
    assert_eq!(
        probe(&["--no-default-features"], "f_none"),
        "",
        "с --no-default-features не должно быть включено ничего"
    );
}

#[test]
fn color_feature_adds_color() {
    assert_eq!(probe(&["--features", "color"], "f_color"), "json,color", "color должна добавляться к json");
    assert_eq!(
        probe(&["--no-default-features", "--features", "color"], "f_color_only"),
        "color",
        "color не должна тянуть за собой json"
    );
}

#[test]
fn all_features_build() {
    assert_eq!(probe(&["--all-features"], "f_all"), "json,color", "с --all-features должны собраться обе");
}

#[test]
fn colorize_is_not_a_public_feature() {
    let out = cargo(&["build", "--quiet"], &["--features", "colorize"], "f_hidden");
    assert!(
        !out.status.success(),
        "feature `colorize` не должна существовать: зависимость включают через `dep:colorize`, а публичное имя feature называется color"
    );
}

#[test]
fn colorize_is_built_only_with_color() {
    assert!(!tree_has(&["--no-default-features"], "colorize"), "без color крейт colorize не должен попадать в дерево");
    assert!(tree_has(&["--features", "color"], "colorize"), "с color крейт colorize должен быть в дереве");
}

#[test]
fn formatters_work() {
    assert_eq!(logfmt::format_plain("INFO", "hi"), "[INFO] hi", "простой формат: [УРОВЕНЬ] текст");
    #[cfg(feature = "json")]
    assert_eq!(
        logfmt::format_json("INFO", "say \"hi\""),
        "{\"level\":\"INFO\",\"msg\":\"say \\\"hi\\\"\"}",
        "кавычки в тексте надо экранировать"
    );
}
