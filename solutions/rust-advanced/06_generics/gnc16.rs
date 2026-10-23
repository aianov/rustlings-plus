use std::marker::PhantomData;

pub struct Missing;
pub struct Set;

#[derive(Debug, PartialEq)]
pub struct Profile {
    pub name: String,
    pub email: String,
    pub bio: Option<String>,
}

pub struct ProfileBuilder<N, E> {
    name: Option<String>,
    email: Option<String>,
    bio: Option<String>,
    _marker: PhantomData<(N, E)>,
}

impl ProfileBuilder<Missing, Missing> {
    pub fn new() -> Self {
        ProfileBuilder { name: None, email: None, bio: None, _marker: PhantomData }
    }
}

impl Default for ProfileBuilder<Missing, Missing> {
    fn default() -> Self {
        ProfileBuilder::new()
    }
}

impl<N, E> ProfileBuilder<N, E> {
    pub fn name(self, name: impl Into<String>) -> ProfileBuilder<Set, E> {
        ProfileBuilder { name: Some(name.into()), email: self.email, bio: self.bio, _marker: PhantomData }
    }

    pub fn email(self, email: impl Into<String>) -> ProfileBuilder<N, Set> {
        ProfileBuilder { name: self.name, email: Some(email.into()), bio: self.bio, _marker: PhantomData }
    }

    pub fn bio(mut self, bio: impl Into<String>) -> Self {
        self.bio = Some(bio.into());
        self
    }
}

impl ProfileBuilder<Set, Set> {
    pub fn build(self) -> Profile {
        Profile {
            name: self.name.unwrap_or_default(),
            email: self.email.unwrap_or_default(),
            bio: self.bio,
        }
    }
}

fn main() {
    let p = ProfileBuilder::new().name("ann").email("a@x.io").build();
    println!("{p:?}");
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = include_str!("gnc16.rs");

    fn compiles_with(extra: &str) -> bool {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("rustlings_probe_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).expect("не удалось создать папку");
        let file = dir.join("probe.rs");
        let code = SRC.split("#[cfg(test)]").next().unwrap_or("");
        std::fs::write(&file, format!("{code}\n{extra}\n")).expect("не удалось записать файл");
        let ok = std::process::Command::new("rustc")
            .args(["--edition", "2024", "--crate-type", "lib", "--emit=metadata", "-A", "warnings", "--out-dir"])
            .arg(&dir)
            .arg(&file)
            .output()
            .expect("не удалось запустить rustc")
            .status
            .success();
        let _ = std::fs::remove_dir_all(&dir);
        ok
    }


    #[test]
    fn builds_in_either_order() {
        let a = ProfileBuilder::new().name("ann").email("a@x.io").build();
        let b = ProfileBuilder::new().email("a@x.io").name("ann").build();
        assert_eq!(a, b);
        assert_eq!(a.bio, None);
    }

    #[test]
    fn bio_is_optional_and_does_not_change_the_state() {
        let p = ProfileBuilder::new().bio("hi").name("ann").bio("rustacean").email("a@x.io").build();
        assert_eq!(p.bio.as_deref(), Some("rustacean"), "последнее значение побеждает");
    }

    #[test]
    fn setters_can_repeat() {
        let p = ProfileBuilder::new().name("x").name("ann").email("old").email("new@x.io").build();
        assert_eq!((p.name.as_str(), p.email.as_str()), ("ann", "new@x.io"));
    }

    #[test]
    fn build_needs_both_fields() {
        assert!(compiles_with("fn p() -> Profile { ProfileBuilder::new().name(\"a\").email(\"b\").build() }"), "с обоими полями собирается");
        assert!(!compiles_with("fn p() -> Profile { ProfileBuilder::new().build() }"), "пустой builder собираться не должен");
        assert!(!compiles_with("fn p() -> Profile { ProfileBuilder::new().name(\"a\").build() }"), "без почты build недоступен");
        assert!(!compiles_with("fn p() -> Profile { ProfileBuilder::new().email(\"a\").bio(\"b\").build() }"), "без имени тоже, даже после bio");
    }
}
