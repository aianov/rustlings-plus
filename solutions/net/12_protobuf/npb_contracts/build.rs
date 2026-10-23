fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto");
    let mut config = prost_build::Config::new();
    config.btree_map(["."]);
    config.compile_protos(&["proto/social/v1/social.proto"], &["proto"])?;
    Ok(())
}
