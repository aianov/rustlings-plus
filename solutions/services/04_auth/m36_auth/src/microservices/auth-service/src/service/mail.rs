#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Letter {
    pub to: String,
    pub subject: String,
    pub link: String,
}

pub trait Mailer: Send + Sync {
    fn send(&self, letter: Letter);
}
