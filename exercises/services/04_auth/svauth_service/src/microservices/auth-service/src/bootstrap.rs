use crate::controller::AuthApp;
use crate::service::{AuthService, Config};

pub fn build(config: Config) -> AuthApp {
    AuthApp::new(AuthService::new(config))
}
