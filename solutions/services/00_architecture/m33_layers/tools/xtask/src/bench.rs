use std::path::Path;
use std::process::Command;

use crate::root;

const ARGS: [&str; 7] = [
    "run",
    "--release",
    "--quiet",
    "--package",
    "post-service",
    "--example",
    "feed_bench",
];

pub fn run(root: &Path) -> Result<(), String> {
    let cargo = root::cargo();
    let status = Command::new(&cargo)
        .args(ARGS)
        .current_dir(root)
        .status()
        .map_err(|e| format!("не удалось запустить {cargo}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("замер `cargo {}` упал: {status}", ARGS.join(" ")))
    }
}
