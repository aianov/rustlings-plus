use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn cargo(sub: &[&str], target: &str) -> Output {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(target);
    let _ = std::fs::remove_dir_all(&dir);
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(sub)
        .arg("--target-dir")
        .arg(dir)
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .output()
        .expect("не удалось запустить cargo")
}

fn rustc_line(build_log: &str, crate_name: &str) -> String {
    build_log
        .lines()
        .find(|l| l.contains("Running") && l.contains(&format!("--crate-name {crate_name} ")))
        .unwrap_or_else(|| panic!("в логе сборки нет запуска rustc для крейта {crate_name}:\n{build_log}"))
        .to_string()
}

fn build_log(sub: &[&str], target: &str) -> String {
    let out = cargo(sub, target);
    let log = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(out.status.success(), "сборка упала:\n{log}");
    log
}

#[test]
fn dev_profile_is_lightly_optimized() {
    let log = build_log(&["build", "-v"], "p_dev");
    assert!(
        rustc_line(&log, "profile_lab").contains("-C opt-level=1"),
        "в [profile.dev] должен стоять opt-level = 1"
    );
}

#[test]
fn heavy_dependency_is_fully_optimized_even_in_dev() {
    let log = build_log(&["build", "-v"], "p_dev_heavy");
    let line = rustc_line(&log, "heavy");
    assert!(
        line.contains("-C opt-level=3"),
        "крейт heavy в dev должен собираться с opt-level = 3: нужна секция [profile.dev.package.heavy]"
    );
    assert!(
        rustc_line(&log, "profile_lab").contains("-C opt-level=1"),
        "настройка для heavy не должна менять уровень оптимизации основного пакета"
    );
}

#[test]
fn release_profile_is_tuned() {
    let log = build_log(&["build", "--release", "-v"], "p_release");
    let line = rustc_line(&log, "profile_lab");
    assert!(line.contains("-C lto"), "в [profile.release] нужна lto = true");
    assert!(line.contains("-C codegen-units=1"), "в [profile.release] нужна codegen-units = 1");
    assert!(line.contains("-C panic=abort"), "в [profile.release] нужна panic = \"abort\"");
}

#[test]
fn release_keeps_overflow_checks() {
    let out = cargo(&["run", "--release", "--quiet", "--bin", "probe", "--", "255"], "p_release_run");
    assert!(
        !out.status.success(),
        "в release 255 + 1 в u8 должно приводить к аварийному завершению: нужна overflow-checks = true"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("overflow"), "ожидали сообщение о переполнении, а получили:\n{stderr}");
}

#[test]
fn release_still_computes_the_same_answer() {
    let out = cargo(&["run", "--release", "--quiet", "--bin", "profile-lab"], "p_release_main");
    assert!(out.status.success(), "profile-lab в release упал");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim().len(),
        16,
        "profile-lab должен печатать 16 шестнадцатеричных цифр"
    );
}
