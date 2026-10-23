use contracts::{PostId, UserId};

#[derive(Clone, Debug, PartialEq)]
pub struct Post {
    pub id: PostId,
    pub author: UserId,
    pub text: String,
}
