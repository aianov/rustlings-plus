fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto");
    tonic_prost_build::configure().compile_protos(
        &[
            "proto/social/v1/common.proto",
            "proto/social/v1/auth.proto",
            "proto/social/v1/users.proto",
            "proto/social/v1/posts.proto",
        ],
        &["proto"],
    )?;
    Ok(())
}
