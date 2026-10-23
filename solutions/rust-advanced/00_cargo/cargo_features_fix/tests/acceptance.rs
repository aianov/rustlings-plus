use std::path::{Path, PathBuf};
use std::process::Command;

fn probe(flags: &[&str], target: &str) -> Vec<String> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(target);
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["run", "--quiet", "--example", "probe", "--target-dir"])
        .arg(dir)
        .args(flags)
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .output()
        .expect("не удалось запустить cargo");
    assert!(
        out.status.success(),
        "пример probe не собрался с флагами {flags:?}:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .split(',')
        .map(str::to_string)
        .collect()
}

#[test]
fn default_build_has_logging() {
    assert_eq!(probe(&[], "g_default"), ["get", "put", "log"], "по умолчанию кэш должен уметь get, put и log");
}

#[test]
fn logging_can_be_switched_off_by_disabling_defaults() {
    assert_eq!(
        probe(&["--no-default-features"], "g_none"),
        ["get", "put"],
        "без default features логирования быть не должно: выключать надо отказом от default, а не отдельной feature"
    );
}

#[test]
fn stats_feature_builds() {
    assert_eq!(
        probe(&["--features", "stats"], "g_stats"),
        ["get", "put", "log", "stats"],
        "с feature stats библиотека должна собираться: ей нужен крейт tinymap"
    );
}

#[test]
fn enabling_everything_never_removes_anything() {
    let default = probe(&[], "g_default2");
    let all = probe(&["--all-features"], "g_all");
    for cap in &default {
        assert!(
            all.contains(cap),
            "с --all-features пропала возможность `{cap}`: features должны только добавлять, иначе включение одной ломает соседей"
        );
    }
}

#[test]
fn optional_dependency_is_not_a_public_feature() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("g_hidden");
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["build", "--quiet", "--features", "tinymap", "--target-dir"])
        .arg(dir)
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .output()
        .expect("не удалось запустить cargo");
    assert!(
        !out.status.success(),
        "feature `tinymap` не должна существовать: зависимость включают записью `dep:tinymap`, она прячет неявную feature"
    );
}
