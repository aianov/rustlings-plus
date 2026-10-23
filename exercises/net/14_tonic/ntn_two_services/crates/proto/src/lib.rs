pub mod users {
    pub mod v1 {
        tonic::include_proto!("users.v1");
    }
}

pub mod posts {
    pub mod v1 {
        tonic::include_proto!("posts.v1");
    }
}
