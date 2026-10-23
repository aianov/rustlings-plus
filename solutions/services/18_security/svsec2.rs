
use std::collections::HashMap;
fn main() {}

pub trait Hasher {
    fn hash(&self, password: &str) -> String;
    fn verify(&self, hash: &str, password: &str) -> bool;
}

#[derive(Debug, PartialEq, Eq)]
pub struct InvalidCredentials;

pub struct Users<H: Hasher> {
    hasher: H,
    accounts: HashMap<String, (u64, String)>,
    dummy: String,
}

impl<H: Hasher> Users<H> {
    pub fn new(hasher: H, accounts: &[(&str, u64, &str)]) -> Users<H> {
        let accounts = accounts.iter().map(|(name, id, pw)| (name.to_string(), (*id, hasher.hash(pw)))).collect();
        let dummy = hasher.hash("пароль, которого ни у кого нет, потому что он случаен: 6c1f9e0b7a");
        Users { hasher, accounts, dummy }
    }

    pub fn login(&self, name: &str, password: &str) -> Result<u64, InvalidCredentials> {
        match self.accounts.get(name) {
            Some((id, hash)) => {
                if self.hasher.verify(hash, password) { Ok(*id) } else { Err(InvalidCredentials) }
            }
            None => {
                let _ = self.hasher.verify(&self.dummy, password);
                Err(InvalidCredentials)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// «Медленный» хешер: считает вызовы проверки, чтобы сравнивать работу, а не секунды.
    struct Counting {
        verifies: AtomicU32,
    }

    impl Hasher for &Counting {
        fn hash(&self, password: &str) -> String {
            format!("slow${password}")
        }
        fn verify(&self, hash: &str, password: &str) -> bool {
            self.verifies.fetch_add(1, Ordering::SeqCst);
            hash == format!("slow${password}")
        }
    }

    fn setup(counter: &Counting) -> Users<&Counting> {
        Users::new(counter, &[("anna", 1, "secret")])
    }

    #[test]
    fn a_correct_login_still_works() {
        let c = Counting { verifies: AtomicU32::new(0) };
        let u = setup(&c);
        assert_eq!(u.login("anna", "secret"), Ok(1));
        assert_eq!(u.login("anna", "wrong"), Err(InvalidCredentials));
    }

    #[test]
    fn every_login_costs_exactly_one_verification() {
        let c = Counting { verifies: AtomicU32::new(0) };
        let u = setup(&c);
        for (name, pw) in [("anna", "secret"), ("anna", "wrong"), ("ghost", "secret"), ("ghost", ""), ("", "x")] {
            let before = c.verifies.load(Ordering::SeqCst);
            let _ = u.login(name, pw);
            let spent = c.verifies.load(Ordering::SeqCst) - before;
            assert_eq!(spent, 1, "вход ({name:?}, {pw:?}) выполнил {spent} проверок пароля: по числу проверок (то есть по времени) видно, существует ли имя");
        }
    }

    #[test]
    fn an_unknown_user_never_gets_in() {
        let c = Counting { verifies: AtomicU32::new(0) };
        let u = setup(&c);
        for pw in ["", "dummy", "password", "secret", "slow$", "пароль", "x"] {
            assert_eq!(u.login("ghost", pw), Err(InvalidCredentials), "несуществующее имя вошло с паролем {pw:?}");
        }
    }
}
