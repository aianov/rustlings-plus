fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto");
    prost_build::compile_protos(&["proto/social/v1/common.proto", "proto/social/v1/post.proto"], &["proto"])?;
    Ok(())
}
