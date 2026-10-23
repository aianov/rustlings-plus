use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "usage: cargo xtask <check | new-service <name>>";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["check"] => check(),
        ["new-service", name] => new_service(name),
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn cargo(args: &[&str]) -> Result<(), String> {
    let program = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(program)
        .args(args)
        .current_dir(root())
        .status()
        .map_err(|e| format!("не удалось запустить cargo: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`cargo {}` завершилась с ошибкой", args.join(" ")))
    }
}

fn check() -> Result<(), String> {
    cargo(&["fmt", "--all", "--check"])?;
    cargo(&[
        "clippy",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ])?;
    cargo(&["test", "--workspace"])
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn new_service(name: &str) -> Result<(), String> {
    if !valid_name(name) {
        return Err(format!(
            "плохое имя `{name}`: маленькие латинские буквы, цифры и дефис, начинать с буквы"
        ));
    }
    let dir = root().join("services").join(name);
    if dir.exists() {
        return Err(format!("services/{name} уже существует"));
    }
    let io = |e: std::io::Error| format!("services/{name}: {e}");
    fs::create_dir_all(dir.join("src")).map_err(io)?;
    let manifest =
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n");
    fs::write(dir.join("Cargo.toml"), manifest).map_err(io)?;
    let main = format!("fn main() {{\n    println!(\"{name} started\");\n}}\n");
    fs::write(dir.join("src/main.rs"), main).map_err(io)?;
    println!("created services/{name}");
    Ok(())
}
