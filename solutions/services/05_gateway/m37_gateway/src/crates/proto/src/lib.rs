mod endpoints;
mod status;

pub mod social {
    pub mod v1 {
        tonic::include_proto!("social.v1");
    }
}

pub use endpoints::{AUTH, AUTH_HTTP, AVATAR_MAX, CHAT, Endpoint, MESSAGE_MAX, POSTS, USERS};
pub use status::{
    ERROR_CODE, ERROR_DETAILS, REQUEST_ID, domain, domain_code, explain, from_status, grpc_code,
    internal, invalid, is_request_id, new_request_id, request_id, to_status, traced,
};
