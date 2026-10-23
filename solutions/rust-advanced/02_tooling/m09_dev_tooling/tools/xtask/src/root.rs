use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub fn find() -> Result<PathBuf, String> {
    let here = env::current_dir().map_err(|e| format!("не узнать текущую папку: {e}"))?;
    here.ancestors()
        .find(|dir| is_workspace_root(dir))
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            format!(
                "{} не внутри воркспейса: выше нет Cargo.toml с [workspace]",
                here.display()
            )
        })
}

fn is_workspace_root(dir: &Path) -> bool {
    fs::read_to_string(dir.join("Cargo.toml"))
        .is_ok_and(|text| text.lines().any(|line| line.trim() == "[workspace]"))
}
