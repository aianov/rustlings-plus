use contracts::UserId;

pub const MAX_MESSAGE_LEN: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub from: UserId,
    pub to: UserId,
    pub text: String,
}
