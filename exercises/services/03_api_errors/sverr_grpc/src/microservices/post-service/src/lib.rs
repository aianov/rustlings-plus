pub mod bootstrap;
pub mod controller;
pub mod domain;
pub mod errors;

pub mod pb {
    tonic::include_proto!("post.v1");
}
