pub mod common;
pub mod user_client;

pub mod pb {
    tonic::include_proto!("user.v1");
}
