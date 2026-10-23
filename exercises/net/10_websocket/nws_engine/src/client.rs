use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::base64;
use crate::frame::{MaskGen, Role};
use crate::handshake::{WsConfig, accept_key};
use crate::stream::{WsError, WsStream};

pub fn new_key(keys: &mut MaskGen) -> String {
    todo!()
}

pub fn client_request(host: &str, path: &str, key: &str, origin: Option<&str>) -> String {
    todo!()
}

pub fn verify_response(head: &str, key: &str) -> Result<(), String> {
    todo!()
}


pub async fn connect(addr: SocketAddr, host: &str, path: &str, origin: Option<&str>, config: &WsConfig) -> Result<WsStream<TcpStream>, WsError> {
    todo!()
}
