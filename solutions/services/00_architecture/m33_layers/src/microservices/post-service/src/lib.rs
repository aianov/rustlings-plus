pub mod bootstrap;
pub mod client;
pub mod common;
pub mod controller;
pub mod dto;
pub mod repository;
pub mod service;

pub use bootstrap::{GATEWAY, run};
pub use client::GrpcUsers;
pub use common::{Config, Error, JOURNAL_ENV, Result};
pub use controller::{PostsController, RECENT_MAX, SYNC_BUDGET};
pub use repository::MemoryPosts;
pub use service::{PostError, PostService, UserDirectory};
