fn main() {
    let name = std::env::var("CARGO_PKG_NAME").expect("cargo задаёт CARGO_PKG_NAME");
    let version = std::env::var("CARGO_PKG_VERSION").expect("cargo задаёт CARGO_PKG_VERSION");
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rustc-env=REPORT_VERSION={}", stamp::version_line(&name, &version));
}
