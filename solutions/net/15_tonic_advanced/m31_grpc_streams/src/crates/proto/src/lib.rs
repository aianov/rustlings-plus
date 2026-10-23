mod endpoints;
mod status;

pub mod social {
    pub mod v1 {
        tonic::include_proto!("social.v1");
    }
}

pub use endpoints::{AUTH, AVATAR_MAX, CHAT, Endpoint, MESSAGE_MAX, POSTS, USERS};
pub use status::{
    ERROR_CODE, REQUEST_ID, domain, domain_code, explain, internal, new_request_id, request_id,
    traced,
};
