use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use proto::social::v1::auth_server::{Auth, AuthServer};
use proto::social::v1::users_server::{Users, UsersServer};
use proto::social::v1::{
    Account, Avatar, AvatarRequest, Changed, CreateProfileRequest, FindProfileRequest,
    GetAccountRequest, GetProfileRequest, ListProfilesRequest, PairRequest, Profile, ProfileList,
    PutAvatarReply, PutAvatarRequest, RelationsReply, RelationsRequest, ValidateTokenRequest,
};
use proto::{domain, request_id};
use service_runtime::CancellationToken;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use crate::support::listener;

pub const LEAK: &str = "пароль hunter2 от базы profiles";

#[derive(Clone, Default)]
pub struct Calls {
    seen: Arc<Mutex<Vec<(String, String)>>>,
}

impl Calls {
    fn record<T>(&self, method: &str, request: &Request<T>) {
        let rid = request_id(request.metadata());
        let mut seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        seen.push((method.to_string(), rid));
    }

    pub fn take(&self) -> Vec<(String, String)> {
        std::mem::take(&mut *self.seen.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

pub struct FakeUsers {
    pub calls: Calls,
}

const NAMES: [&str; 2] = ["alice", "bob"];

fn profile(id: u64) -> Option<Profile> {
    let name = NAMES.get(usize::try_from(id).ok()?.checked_sub(1)?)?;
    Some(Profile {
        id,
        username: name.to_string(),
    })
}

#[tonic::async_trait]
impl Users for FakeUsers {
    async fn create_profile(
        &self,
        request: Request<CreateProfileRequest>,
    ) -> Result<Response<Profile>, Status> {
        self.calls.record("CreateProfile", &request);
        Err(Status::unimplemented("фейк не регистрирует"))
    }

    async fn get_profile(
        &self,
        request: Request<GetProfileRequest>,
    ) -> Result<Response<Profile>, Status> {
        self.calls.record("GetProfile", &request);
        let id = request.into_inner().user_id;
        profile(id)
            .map(Response::new)
            .ok_or_else(|| domain("not_found", &format!("пользователь {id} не найден")))
    }

    async fn find_profile(
        &self,
        request: Request<FindProfileRequest>,
    ) -> Result<Response<Profile>, Status> {
        self.calls.record("FindProfile", &request);
        let name = request.into_inner().username;
        if name == "leaky" {
            return Err(Status::internal(LEAK));
        }
        (1..=2)
            .filter_map(profile)
            .find(|profile| profile.username == name)
            .map(Response::new)
            .ok_or_else(|| domain("not_found", &format!("пользователя @{name} нет")))
    }

    async fn list_profiles(
        &self,
        request: Request<ListProfilesRequest>,
    ) -> Result<Response<ProfileList>, Status> {
        self.calls.record("ListProfiles", &request);
        let after = request.into_inner().after;
        let profiles = (after + 1..=2).filter_map(profile).collect();
        Ok(Response::new(ProfileList { profiles }))
    }

    async fn follow(&self, request: Request<PairRequest>) -> Result<Response<Changed>, Status> {
        self.calls.record("Follow", &request);
        Ok(Response::new(Changed { changed: true }))
    }

    async fn mute(&self, request: Request<PairRequest>) -> Result<Response<Changed>, Status> {
        self.calls.record("Mute", &request);
        Ok(Response::new(Changed { changed: true }))
    }

    async fn relations(
        &self,
        request: Request<RelationsRequest>,
    ) -> Result<Response<RelationsReply>, Status> {
        self.calls.record("Relations", &request);
        let reply = match request.into_inner().user_id {
            1 => RelationsReply {
                followers: vec![2],
                audience: vec![2],
                ..RelationsReply::default()
            },
            _ => RelationsReply::default(),
        };
        Ok(Response::new(reply))
    }

    async fn put_avatar(
        &self,
        request: Request<PutAvatarRequest>,
    ) -> Result<Response<PutAvatarReply>, Status> {
        self.calls.record("PutAvatar", &request);
        Err(Status::unimplemented("фейк без аватаров"))
    }

    async fn get_avatar(
        &self,
        request: Request<AvatarRequest>,
    ) -> Result<Response<Avatar>, Status> {
        self.calls.record("GetAvatar", &request);
        Err(Status::unimplemented("фейк без аватаров"))
    }

    async fn delete_avatar(
        &self,
        request: Request<AvatarRequest>,
    ) -> Result<Response<Changed>, Status> {
        self.calls.record("DeleteAvatar", &request);
        Err(Status::unimplemented("фейк без аватаров"))
    }
}

pub struct FakeAuth;

#[tonic::async_trait]
impl Auth for FakeAuth {
    async fn validate_token(
        &self,
        request: Request<ValidateTokenRequest>,
    ) -> Result<Response<Account>, Status> {
        let token = request.into_inner().token;
        let id = match token.as_str() {
            "alice-token" => 1,
            "bob-token" => 2,
            _ => return Err(domain("unauthorized", "токен неизвестен")),
        };
        let account = profile(id).map(|profile| Account {
            user_id: profile.id,
            username: profile.username,
        });
        account
            .map(Response::new)
            .ok_or_else(|| Status::internal("фейк сломан"))
    }

    async fn get_account(
        &self,
        request: Request<GetAccountRequest>,
    ) -> Result<Response<Account>, Status> {
        let id = request.into_inner().user_id;
        let account = profile(id).map(|profile| Account {
            user_id: profile.id,
            username: profile.username,
        });
        account
            .map(Response::new)
            .ok_or_else(|| domain("not_found", "нет такой учётной записи"))
    }
}

pub async fn fake_users(stop: &CancellationToken) -> (SocketAddr, Calls) {
    let (listener, addr) = listener().await;
    let calls = Calls::default();
    let users = UsersServer::new(FakeUsers {
        calls: calls.clone(),
    });
    let stop = stop.clone();
    tokio::spawn(async move {
        Server::builder()
            .add_service(users)
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
                stop.cancelled().await
            })
            .await
    });
    (addr, calls)
}

pub async fn fake_auth(stop: &CancellationToken) -> SocketAddr {
    let (listener, addr) = listener().await;
    let stop = stop.clone();
    tokio::spawn(async move {
        Server::builder()
            .add_service(AuthServer::new(FakeAuth))
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
                stop.cancelled().await
            })
            .await
    });
    addr
}
