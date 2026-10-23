use contracts::UserId;
use proto::social::v1::{Avatar as AvatarReply, Profile, RelationsReply};

use crate::common::{Avatar, Relations, User};

pub fn profile(user: &User) -> Profile {
    Profile {
        id: user.id.0,
        username: user.name.as_ref().to_string(),
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
