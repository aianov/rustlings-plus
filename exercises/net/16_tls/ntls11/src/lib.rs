// Личность клиента берётся из его сертификата, а права проверяются по таблице. Допиши все три части.
#![allow(unused_variables)]

use rustls::ServerConfig;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

pub fn identity_of(cert_der: &[u8]) -> Option<String> {
    todo!()
}

pub struct Rules {
    by_method: HashMap<String, Vec<String>>,
}

impl Rules {
    pub fn new(table: &[(&str, &[&str])]) -> Rules {
        Rules { by_method: table.iter().map(|(m, who)| (m.to_string(), who.iter().map(|w| w.to_string()).collect())).collect() }
    }

    pub fn allows(&self, identity: Option<&str>, method: &str) -> bool {
        todo!()
    }
}

pub async fn serve(listener: TcpListener, config: Arc<ServerConfig>, rules: Arc<Rules>) {
    todo!()
}
