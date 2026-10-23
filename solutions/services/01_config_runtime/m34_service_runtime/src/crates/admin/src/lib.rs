mod routes;
mod server;

pub use routes::{ready_view, router};
pub use server::{Admin, admin_config, serve};
