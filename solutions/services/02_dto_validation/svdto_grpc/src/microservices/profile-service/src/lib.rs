pub mod bootstrap;
pub mod controller;
pub mod domain;

pub mod pb {
    tonic::include_proto!("profile.v1");
}
