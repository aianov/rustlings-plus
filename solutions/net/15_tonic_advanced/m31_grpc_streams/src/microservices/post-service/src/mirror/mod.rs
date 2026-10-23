use grpc_kit::Stamp;
use proto::social::v1::users_client::UsersClient;
use proto::social::v1::{GetProfileRequest, ListProfilesRequest, Profile, RelationsRequest};
use proto::{domain, explain, internal};
use social_core::{App, DomainError, UserId, report};
use tokio::sync::Mutex;
use tonic::Status;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Channel;

type Users = UsersClient<InterceptedService<Channel, Stamp>>;

pub struct Mirror {
    users: Channel,
    sync: Mutex<()>,
}

pub(crate) fn failed(error: impl Into<social_core::Error>, rid: &str) -> Status {
    let error = error.into();
    let code = error.code();
    if code == "internal" {
        eprintln!("post-service [{rid}]: {}", report(&error));
    }
    domain(code, &report(&error))
}

fn upstream(status: Status, rid: &str) -> Status {
    let (code, message) = explain(&status, "сервис пользователей");
    if code == "internal" {
        eprintln!("post-service [{rid}]: user-service ответил {status}");
    }
    domain(&code, &message)
}

impl Mirror {
    pub fn new(users: Channel) -> Mirror {
        Mirror {
            users,
            sync: Mutex::new(()),
        }
    }

    fn users(&self, stamp: &Stamp) -> Users {
        UsersClient::with_interceptor(self.users.clone(), stamp.clone())
    }

    pub async fn profile(&self, id: UserId, stamp: &Stamp) -> Result<Profile, Status> {
        let ask = GetProfileRequest { user_id: id.0 };
        let answer = self.users(stamp).get_profile(ask).await;
        answer
            .map(tonic::Response::into_inner)
            .map_err(|status| upstream(status, stamp.request_id()))
    }

    pub async fn ensure(&self, app: &App, id: UserId, stamp: &Stamp) -> Result<(), Status> {
        if app.user(id).await.is_some() {
            return Ok(());
        }
        let rid = stamp.request_id();
        let _sync = self.sync.lock().await;
        loop {
            let known = app.user_count().await as u64;
            if known >= id.0 {
                return Ok(());
            }
            let ask = ListProfilesRequest {
                after: known,
                limit: 0,
            };
            let page = self.users(stamp).list_profiles(ask).await;
            let page = page.map_err(|status| upstream(status, rid))?.into_inner();
            if page.profiles.is_empty() {
                return Err(failed(DomainError::UnknownUser(id), rid));
            }
            for profile in page.profiles {
                let mirrored = app.register(&profile.username).await;
                let mirrored = mirrored.map_err(|error| failed(error, rid))?;
                if mirrored.0 != profile.id {
                    eprintln!(
                        "post-service [{rid}]: зеркало профилей разошлось, у нас {}, в user-service {}",
                        mirrored.0, profile.id
                    );
                    return Err(internal());
                }
            }
        }
    }

    pub async fn sync_viewer(
        &self,
        app: &App,
        viewer: UserId,
        stamp: &Stamp,
    ) -> Result<(), Status> {
        let rid = stamp.request_id();
        let ask = RelationsRequest { user_id: viewer.0 };
        let relations = self.users(stamp).relations(ask).await;
        let relations = relations
            .map_err(|status| upstream(status, rid))?
            .into_inner();
        let newest = relations.following.iter().chain(&relations.muted).max();
        if let Some(&newest) = newest {
            self.ensure(app, UserId(newest), stamp).await?;
        }
        for author in relations.following {
            let followed = app.follow(viewer, UserId(author)).await;
            followed.map_err(|error| failed(error, rid))?;
        }
        for author in relations.muted {
            let muted = app.mute(viewer, UserId(author)).await;
            muted.map_err(|error| failed(error, rid))?;
        }
        Ok(())
    }
}
