use proto::social::v1::users_client::UsersClient;
use proto::social::v1::{GetProfileRequest, ListProfilesRequest, Profile, RelationsRequest};
use proto::{domain, explain, internal, traced};
use social_core::{App, DomainError, UserId, report};
use tokio::sync::Mutex;
use tonic::Status;
use tonic::transport::Channel;

pub struct Mirror {
    users: UsersClient<Channel>,
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
            users: UsersClient::new(users),
            sync: Mutex::new(()),
        }
    }

    pub async fn profile(&self, id: UserId, rid: &str) -> Result<Profile, Status> {
        let ask = traced(GetProfileRequest { user_id: id.0 }, rid);
        let answer = self.users.clone().get_profile(ask).await;
        answer
            .map(tonic::Response::into_inner)
            .map_err(|status| upstream(status, rid))
    }

    pub async fn ensure(&self, app: &App, id: UserId, rid: &str) -> Result<(), Status> {
        if app.user(id).await.is_some() {
            return Ok(());
        }
        let _sync = self.sync.lock().await;
        loop {
            let known = app.user_count().await as u64;
            if known >= id.0 {
                return Ok(());
            }
            let ask = traced(
                ListProfilesRequest {
                    after: known,
                    limit: 0,
                },
                rid,
            );
            let page = self.users.clone().list_profiles(ask).await;
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

    pub async fn sync_viewer(&self, app: &App, viewer: UserId, rid: &str) -> Result<(), Status> {
        let ask = traced(RelationsRequest { user_id: viewer.0 }, rid);
        let relations = self.users.clone().relations(ask).await;
        let relations = relations
            .map_err(|status| upstream(status, rid))?
            .into_inner();
        let newest = relations.following.iter().chain(&relations.muted).max();
        if let Some(&newest) = newest {
            self.ensure(app, UserId(newest), rid).await?;
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
