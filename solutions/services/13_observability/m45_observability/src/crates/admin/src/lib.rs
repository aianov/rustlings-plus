mod routes;
mod server;

pub use routes::{METRICS_TYPE, ready_view, router};
pub use server::{Admin, admin_config, serve};
