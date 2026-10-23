pub mod social {
    pub mod v1 {
        include!(concat!(env!("OUT_DIR"), "/social.v1.rs"));
    }
}
