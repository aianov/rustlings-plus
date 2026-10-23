use serde::{Deserialize, Serialize};

fn parse_id(prefix: &str, kind: &str, text: &str) -> Result<u64, String> {
    text.strip_prefix(prefix)
        .and_then(|digits| digits.parse().ok())
        .ok_or_else(|| format!("invalid {kind} id {text:?}: expected {prefix}<number>"))
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct UserId(pub u64);

impl From<UserId> for String {
    fn from(id: UserId) -> String {
        format!("u_{}", id.0)
    }
}

impl TryFrom<String> for UserId {
    type Error = String;

    fn try_from(text: String) -> Result<UserId, String> {
        parse_id("u_", "user", &text).map(UserId)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct PostId(pub u64);

impl From<PostId> for String {
    fn from(id: PostId) -> String {
        format!("p_{}", id.0)
    }
}

impl TryFrom<String> for PostId {
    type Error = String;

    fn try_from(text: String) -> Result<PostId, String> {
        parse_id("p_", "post", &text).map(PostId)
    }
}
