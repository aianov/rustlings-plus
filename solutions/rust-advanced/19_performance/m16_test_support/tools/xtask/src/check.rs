use std::path::Path;
use std::process::Command;

use crate::root;

const STEPS: [&[&str]; 3] = [
    &["fmt", "--all", "--check"],
    &[
        "clippy",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ],
    &["test", "--workspace"],
];

pub fn run(root: &Path) -> Result<(), String> {
    let cargo = root::cargo();
    for step in STEPS {
        let status = Command::new(&cargo)
            .args(step)
            .current_dir(root)
            .status()
            .map_err(|e| format!("не удалось запустить {cargo}: {e}"))?;
        if !status.success() {
            return Err(format!("шаг `cargo {}` упал: {status}", step.join(" ")));
        }
    }
    Ok(())
}
