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

fn field_patch<T>(
    name: &str,
    sent: Option<T>,
    clear: &[String],
    problems: &mut Vec<String>,
) -> Patch<T> {
    let cleared = clear.iter().any(|c| c == name);
    match (sent, cleared) {
        (Some(_), true) => {
            problems.push(format!("{name}: conflict"));
            Patch::Missing
        }
        (Some(value), false) => Patch::Value(value),
        (None, true) => Patch::Null,
        (None, false) => Patch::Missing,
    }
}

fn to_patch(request: UpdateProfileRequest) -> Result<PatchSet, Status> {
    let mut problems = Vec::new();
    let clear = request.clear_fields;
    for name in &clear {
        if !["displayName", "bio", "birthYear"].contains(&name.as_str()) {
            problems.push(format!("{name}: unknown_field"));
        }
    }
    let patch = PatchSet {
        display_name: field_patch("displayName", request.display_name, &clear, &mut problems),
        bio: field_patch("bio", request.bio, &clear, &mut problems),
        birth_year: field_patch("birthYear", request.birth_year, &clear, &mut problems),
    };
    if problems.is_empty() {
        Ok(patch)
    } else {
        Err(Status::invalid_argument(problems.join("; ")))
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
        let id = request.user_id;
        let patch = to_patch(request)?;
        match self.service.apply(id, patch) {
            Ok(profile) => Ok(Response::new(to_message(profile))),
            Err(UpdateError::NotFound) => Err(Status::not_found("профиль не найден")),
            Err(UpdateError::Invalid(errors)) => {
                let message: Vec<String> = errors.iter().map(|e| format!("{}: {}", e.field, e.code)).collect();
                Err(Status::invalid_argument(message.join("; ")))
            }
        }
    }
}
