use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn cargo(args: &[&str]) -> Output {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("anatomy");
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(args)
        .arg("--target-dir")
        .arg(target)
        .current_dir(root())
        .output()
        .expect("не удалось запустить cargo")
}

fn metadata() -> String {
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root())
        .output()
        .expect("не удалось запустить cargo metadata");
    assert!(
        out.status.success(),
        "cargo не смог прочитать манифест:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("metadata не в UTF-8")
}

fn has_target(meta: &str, kind: &str, name: &str) -> bool {
    let crate_type = if kind == "lib" { "lib" } else { "bin" };
    meta.contains(&format!(
        "\"kind\":[\"{kind}\"],\"crate_types\":[\"{crate_type}\"],\"name\":\"{name}\""
    ))
}

#[test]
fn edition_is_2024() {
    let meta = metadata();
    assert!(
        meta.contains("\"edition\":\"2024\""),
        "в манифесте не стоит edition 2024: код библиотеки использует цепочку let ... && ..., а она есть только в 2024"
    );
    assert!(
        !meta.contains("\"edition\":\"2021\""),
        "где-то ещё остался edition 2021: проверь [package] и все [[bin]]/[lib]"
    );
}

#[test]
fn minimum_rust_version_is_declared() {
    assert!(
        metadata().contains("\"rust_version\":\"1.88\""),
        "в [package] нет rust-version = \"1.88\": без неё cargo не предупредит о слишком старом компиляторе"
    );
}

#[test]
fn library_target_has_underscore_name() {
    assert!(
        has_target(&metadata(), "lib", "tiny_calc"),
        "не вижу библиотеку tiny_calc: у пакета tiny-calc крейт-библиотека называется с подчёркиванием"
    );
}

#[test]
fn main_binary_keeps_package_name() {
    assert!(
        has_target(&metadata(), "bin", "tiny-calc"),
        "не вижу бинарник tiny-calc: src/main.rs всегда получает имя пакета"
    );
}

#[test]
fn second_binary_is_named_calc_bench() {
    let meta = metadata();
    assert!(
        has_target(&meta, "bin", "calc-bench"),
        "не вижу бинарник calc-bench: файл src/bin/bench.rs по умолчанию получает имя bench, переименуй его через [[bin]] с path"
    );
    assert!(
        !has_target(&meta, "bin", "bench"),
        "бинарник bench остался: после [[bin]] name = \"calc-bench\" старого имени быть не должно"
    );
}

#[test]
fn example_and_test_targets_are_discovered() {
    let meta = metadata();
    assert!(has_target(&meta, "example", "demo"), "не вижу example demo из папки examples/");
    assert!(has_target(&meta, "test", "acceptance"), "не вижу интеграционный тест acceptance из папки tests/");
}

#[test]
fn calculator_works() {
    let Some(exe) = option_env!("CARGO_BIN_EXE_tiny-calc") else {
        panic!("бинарник tiny-calc не собран");
    };
    let out = Command::new(exe).args(["6", "*", "7"]).output().unwrap();
    assert!(out.status.success(), "tiny-calc 6 * 7 завершился с ошибкой");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "42", "tiny-calc 6 * 7 должен напечатать 42");
}

#[test]
fn bench_binary_runs() {
    let Some(exe) = option_env!("CARGO_BIN_EXE_calc-bench") else {
        panic!("бинарник calc-bench не собран: проверь имя в [[bin]]");
    };
    let out = Command::new(exe).output().unwrap();
    assert!(out.status.success(), "calc-bench упал");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "ok", "calc-bench должен напечатать ok");
}

#[test]
fn example_runs() {
    let out = cargo(&["run", "--quiet", "--example", "demo"]);
    assert!(
        out.status.success(),
        "cargo run --example demo не собрался:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "Some(42)", "example demo должен напечатать Some(42)");
}
