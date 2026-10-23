// Шлюзу нужен канал до сервиса пользователей: его создают при старте, а сервис в этот момент
// может быть ещё не поднят. Вызов не должен висеть вечно. Допиши make_channel.
#![allow(unused_variables)]

use std::net::SocketAddr;
use std::time::Duration;
use tonic::transport::{Channel, Endpoint};
use tonic::Status;

pub mod pb {
    tonic::include_proto!("users.v1");
}
use pb::users_client::UsersClient;
use pb::GetUserRequest;

pub fn make_channel(addr: SocketAddr, connect_timeout: Duration, request_timeout: Duration) -> Channel {
    todo!()
}

pub async fn handle_of(channel: Channel, id: u64) -> Result<String, Status> {
    let mut client = UsersClient::new(channel);
    let user = client.get_user(GetUserRequest { id }).await?.into_inner();
    Ok(user.handle)
}
