fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto");
    tonic_prost_build::configure().compile_protos(&["proto/users/v1/users.proto", "proto/feed/v1/feed.proto"], &["proto"])?;
    Ok(())
}
