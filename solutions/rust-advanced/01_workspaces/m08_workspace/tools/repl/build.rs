use std::env;

fn main() {
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let profile = env::var("PROFILE").unwrap_or_default();
    println!("cargo::rustc-env=REPL_VERSION=repl {version} ({profile})");
    println!("cargo::rerun-if-changed=build.rs");
}
