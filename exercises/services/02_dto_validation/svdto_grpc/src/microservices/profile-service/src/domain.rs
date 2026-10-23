use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Patch<T> {
    Missing,
    Null,
    Value(T),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub display_name: String,
    pub bio: Option<String>,
    pub birth_year: Option<u16>,
}

#[derive(Debug, PartialEq)]
pub struct PatchSet {
    pub display_name: Patch<String>,
    pub bio: Patch<String>,
    pub birth_year: Patch<u32>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct FieldError {
    pub field: &'static str,
    pub code: &'static str,
}

#[derive(Debug, PartialEq)]
pub enum UpdateError {
    NotFound,
    Invalid(Vec<FieldError>),
}

#[derive(Default)]
pub struct ProfileService {
    profiles: Mutex<HashMap<u64, Profile>>,
}

fn text(
    errors: &mut Vec<FieldError>,
    field: &'static str,
    value: &str,
    max: usize,
) -> Option<String> {
    let value = value.trim();
    let len = value.chars().count();
    if len == 0 {
        errors.push(FieldError { field, code: "too_short" });
        None
    } else if len > max {
        errors.push(FieldError { field, code: "too_long" });
        None
    } else {
        Some(value.to_string())
    }
}

impl ProfileService {
    pub fn with_profiles(seed: Vec<(u64, Profile)>) -> ProfileService {
        ProfileService { profiles: Mutex::new(seed.into_iter().collect()) }
    }

    pub fn get(&self, id: u64) -> Option<Profile> {
        self.profiles.lock().unwrap().get(&id).cloned()
    }

    pub fn apply(&self, id: u64, patch: PatchSet) -> Result<Profile, UpdateError> {
        let mut profiles = self.profiles.lock().unwrap();
        let current = profiles.get(&id).ok_or(UpdateError::NotFound)?;
        let mut next = current.clone();
        let mut errors = Vec::new();
        match patch.display_name {
            Patch::Missing => {}
            Patch::Null => errors.push(FieldError { field: "displayName", code: "null_not_allowed" }),
            Patch::Value(name) => {
                if let Some(name) = text(&mut errors, "displayName", &name, 30) {
                    next.display_name = name;
                }
            }
        }
        match patch.bio {
            Patch::Missing => {}
            Patch::Null => next.bio = None,
            Patch::Value(bio) => {
                if let Some(bio) = text(&mut errors, "bio", &bio, 160) {
                    next.bio = Some(bio);
                }
            }
        }
        match patch.birth_year {
            Patch::Missing => {}
            Patch::Null => next.birth_year = None,
            Patch::Value(year) => match u16::try_from(year) {
                Ok(year) if (1900..=2020).contains(&year) => next.birth_year = Some(year),
                _ => errors.push(FieldError { field: "birthYear", code: "out_of_range" }),
            },
        }
        if !errors.is_empty() {
            return Err(UpdateError::Invalid(errors));
        }
        profiles.insert(id, next.clone());
        Ok(next)
    }
}
