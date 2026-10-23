use std::process::Command;

#[test]
fn version_comes_from_the_build_script() {
    let output = Command::new(env!("CARGO_BIN_EXE_repl"))
        .arg("--version")
        .output()
        .expect("бинарник repl должен запускаться");
    assert!(output.status.success(), "repl --version должен завершаться успешно и не ждать stdin");
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("repl {} ({profile})\n", env!("CARGO_PKG_VERSION")),
        "repl --version печатает строку REPL_VERSION из build.rs: `repl <версия пакета> (<профиль>)`"
    );
}

#[test]
fn version_is_shared_by_the_workspace() {
    assert_eq!(env!("CARGO_PKG_VERSION"), "0.1.0", "версия repl наследуется из [workspace.package] (0.1.0)");
}
