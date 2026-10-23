use std::path::PathBuf;
use std::{env, fs};

fn main() {
    println!("cargo::rerun-if-changed=data/stopwords.txt");
    println!("cargo::rerun-if-changed=build.rs");

    let text = fs::read_to_string("data/stopwords.txt").expect("не удалось прочитать data/stopwords.txt");
    let mut words: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    words.sort_unstable();
    words.dedup();

    let items: String = words.iter().map(|w| format!("    {w:?},\n")).collect();
    let code = format!("const STOPWORDS: &[&str] = &[\n{items}];\n");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("cargo задаёт OUT_DIR"));
    fs::write(out_dir.join("stopwords.rs"), code).expect("не удалось записать stopwords.rs");

    let version = env::var("CARGO_PKG_VERSION").expect("cargo задаёт CARGO_PKG_VERSION");
    println!("cargo::rustc-env=TEXTSTAT_VERSION=textstat {version} ({} stopwords)", words.len());
}
