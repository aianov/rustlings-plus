use crate::domain::{Patch, PatchSet, Profile, ProfileService, UpdateError};
use crate::pb::profiles_server::Profiles;
use crate::pb::{GetProfileRequest, Profile as ProfileMessage, UpdateProfileRequest};
use std::sync::Arc;
use tonic::{Request, Response, Status};

pub struct ProfileController {
    service: Arc<ProfileService>,
}

impl ProfileController {
    pub fn new(service: Arc<ProfileService>) -> ProfileController {
        ProfileController { service }
    }
}

fn to_message(profile: Profile) -> ProfileMessage {
    ProfileMessage {
        display_name: profile.display_name,
        bio: profile.bio.unwrap_or_default(),
        birth_year: profile.birth_year.map(u32::from).unwrap_or_default(),
    }
}

fn to_patch(request: &UpdateProfileRequest) -> PatchSet {
    let present = |text: &str| if text.is_empty() { Patch::Missing } else { Patch::Value(text.to_string()) };
    PatchSet {
        display_name: present(&request.display_name),
        bio: present(&request.bio),
        birth_year: if request.birth_year == 0 { Patch::Missing } else { Patch::Value(request.birth_year) },
    }
}

#[tonic::async_trait]
impl Profiles for ProfileController {
    async fn get_profile(&self, request: Request<GetProfileRequest>) -> Result<Response<ProfileMessage>, Status> {
        let id = request.into_inner().user_id;
        match self.service.get(id) {
            Some(profile) => Ok(Response::new(to_message(profile))),
            None => Err(Status::not_found("профиль не найден")),
        }
    }

    async fn update_profile(&self, request: Request<UpdateProfileRequest>) -> Result<Response<ProfileMessage>, Status> {
        let request = request.into_inner();
        let patch = to_patch(&request);
        match self.service.apply(request.user_id, patch) {
            Ok(profile) => Ok(Response::new(to_message(profile))),
            Err(UpdateError::NotFound) => Err(Status::not_found("профиль не найден")),
            Err(UpdateError::Invalid(_)) => Err(Status::invalid_argument("неверные данные")),
        }
    }
}
