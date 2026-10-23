fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto");
    tonic_prost_build::compile_protos("proto/chat/v1/chat.proto")?;
    Ok(())
}
