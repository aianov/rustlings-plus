#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: u64,
    pub name: String,
}

impl Account {
    pub fn new(id: u64, name: &str) -> Account {
        Account { id, name: name.to_string() }
    }
}
