use std::path::PathBuf;
use std::{env, fs};

fn main() {
    println!("cargo::rerun-if-changed=data/words.txt");
    println!("cargo::rerun-if-changed=build.rs");

    let text = fs::read_to_string("data/words.txt").expect("не удалось прочитать data/words.txt");
    let mut words: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    words.sort_unstable();
    words.dedup();

    let items: String = words.iter().map(|w| format!("    {w:?},\n")).collect();
    let code = format!("pub const WORDS: &[&str] = &[\n{items}];\n");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("cargo задаёт OUT_DIR"));
    fs::write(out_dir.join("words.rs"), code).expect("не удалось записать words.rs");

    let version = env::var("CARGO_PKG_VERSION").expect("cargo задаёт CARGO_PKG_VERSION");
    println!("cargo::rustc-env=WORDLIST_BUILD=wordlist {version} ({} words)", words.len());
}
