use std::time::Duration;

use grpc_kit::Tls;
use http_client::{Client, Config, Error as HttpError, Request, Response};
use tls_edge::mutual_client_config;

use crate::common::{Error, ProxyConfig};

const CONNECT: Duration = Duration::from_secs(1);

const ANSWER_MAX: usize = 64 * 1024;

const AUTH_NAME: &str = "auth-service";

pub enum Relayed {
    Answer(Response),
    Unreachable,
    Late,
}

pub struct AuthHttp {
    client: Client,
    base: String,
}

impl AuthHttp {
    pub fn new(addr: &str, tls: Option<&Tls>, proxy: &ProxyConfig) -> Result<AuthHttp, Error> {
        let secure = match tls {
            Some(tls) => Some(
                mutual_client_config(&tls.ca, &tls.cert, tls.key.expose()).map_err(Error::Certs)?,
            ),
            None => None,
        };
        let scheme = if secure.is_some() { "https" } else { "http" };
        let config = Config {
            connect_timeout: proxy.timeout.min(CONNECT),
            total_timeout: proxy.timeout,
            max_redirects: 0,
            max_body: ANSWER_MAX,
            tls_name: secure.as_ref().map(|_| AUTH_NAME.to_string()),
            tls: secure,
            ..Config::default()
        };
        Ok(AuthHttp {
            client: Client::new(config),
            base: format!("{scheme}://{addr}"),
        })
    }

    pub fn is_secure(&self) -> bool {
        self.base.starts_with("https://")
    }

    pub async fn send(
        &self,
        method: &str,
        target: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
        rid: &str,
    ) -> Relayed {
        let request = Request {
            method: method.to_string(),
            url: format!("{}{target}", self.base),
            headers,
            body: (!body.is_empty()).then_some(body),
        };
        match self.client.relay(request).await {
            Ok(answer) => Relayed::Answer(answer),
            Err(HttpError::TotalTimeout) => {
                tracing::warn!(
                    "api-gateway [{rid}]: auth-service не ответил на {method} {target} вовремя"
                );
                Relayed::Late
            }
            Err(error) => {
                tracing::warn!(
                    "api-gateway [{rid}]: auth-service недоступен для {method} {target}: {error}"
                );
                Relayed::Unreachable
            }
        }
    }
}
