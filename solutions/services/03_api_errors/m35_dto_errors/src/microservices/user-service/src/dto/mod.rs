use contracts::{MAX_NAME_LEN, MIN_NEW_NAME_LEN, UserId, Username};
use proto::social::v1::{
    Avatar as AvatarReply, CreateProfileRequest, Profile, RelationsReply, UpdateProfileRequest,
};
use validation::{CharLen, Invalid, OneOf, Validator, Violation};

use crate::common::{Avatar, Patch, ProfileChange, Relations, User};

pub const CLEARABLE: [&str; 2] = ["display_name", "bio"];

pub fn new_profile(request: &CreateProfileRequest) -> Result<Username, Invalid> {
    let mut check = Validator::new();
    let name = check.parse::<Username>("username", &request.username);
    if let Some(name) = &name {
        let length = CharLen {
            min: MIN_NEW_NAME_LEN,
            max: MAX_NAME_LEN,
        };
        check.check("username", name.as_ref(), &length);
    }
    check.finish_with(name)
}

fn patch<T>(check: &mut Validator, field: &str, value: Option<&str>, clear: bool) -> Patch<T>
where
    T: for<'a> TryFrom<&'a str, Error = Violation>,
{
    match (value, clear) {
        (Some(..), true) => {
            let both = Violation::new("set_and_clear", "поле нельзя сразу задать и очистить");
            check.report(field, both);
            Patch::Keep
        }
        (Some(raw), false) => check.parse(field, raw).map_or(Patch::Keep, Patch::Set),
        (None, true) => Patch::Clear,
        (None, false) => Patch::Keep,
    }
}

pub fn profile_change(request: &UpdateProfileRequest) -> Result<ProfileChange, Invalid> {
    let mut check = Validator::new();
    check.each("clear_fields", &request.clear_fields, |check, field| {
        check.check("", field.as_str(), &OneOf(&CLEARABLE));
    });
    let clear = |name: &str| request.clear_fields.iter().any(|field| field == name);
    let display_name = patch(
        &mut check,
        "display_name",
        request.display_name.as_deref(),
        clear("display_name"),
    );
    let bio = patch(&mut check, "bio", request.bio.as_deref(), clear("bio"));
    check.finish()?;
    Ok(ProfileChange { display_name, bio })
}

pub fn profile(user: &User) -> Profile {
    let text = |value: Option<&str>| value.unwrap_or_default().to_string();
    Profile {
        id: user.id.0,
        username: user.name.as_ref().to_string(),
        display_name: text(user.display_name.as_ref().map(AsRef::as_ref)),
        bio: text(user.bio.as_ref().map(AsRef::as_ref)),
    }
}

fn ids(users: &[UserId]) -> Vec<u64> {
    users.iter().map(|user| user.0).collect()
}

pub fn relations(relations: &Relations) -> RelationsReply {
    RelationsReply {
        following: ids(&relations.following),
        followers: ids(&relations.followers),
        muted: ids(&relations.muted),
        audience: ids(&relations.audience),
    }
}

pub fn avatar(avatar: Avatar) -> AvatarReply {
    AvatarReply {
        content_type: avatar.content_type,
        data: avatar.bytes.as_ref().clone(),
    }
}
