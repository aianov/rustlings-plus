use crate::controller::ProfileController;
use crate::domain::{Profile, ProfileService};
use std::sync::Arc;

pub fn build_with(seed: Vec<(u64, Profile)>) -> ProfileController {
    ProfileController::new(Arc::new(ProfileService::with_profiles(seed)))
}
