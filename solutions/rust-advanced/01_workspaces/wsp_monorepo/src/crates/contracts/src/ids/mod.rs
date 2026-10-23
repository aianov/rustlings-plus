#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserId(pub u64);

impl UserId {
    pub fn parse(text: &str) -> Option<UserId> {
        text.trim().parse().ok().map(UserId)
    }
}
