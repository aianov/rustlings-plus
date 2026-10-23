// Регистрация проверяет ник честно, а вот смена ника забыта: `ProfileService::rename` принимает
// голый `String`, и через неё можно поставить себе ник «  ADMIN !!! ». Тесты ниже написаны так,
// как должен выглядеть правильный код: ник в сервис приходит уже проверенным типом `Username`.
// Цель: сделать так, чтобы непроверенную строку в `rename` нельзя было передать вообще (а не
// просто проверять внутри ещё раз), и чтобы контроллер разбирал сырой ввод на границе.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Username(String);

#[derive(Debug, PartialEq)]
pub struct InvalidUsername;

impl Username {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for Username {
    type Error = InvalidUsername;

    fn try_from(raw: &str) -> Result<Self, InvalidUsername> {
        let name = raw.trim().to_lowercase();
        let ok = (3..=20).contains(&name.chars().count())
            && name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if ok { Ok(Username(name)) } else { Err(InvalidUsername) }
    }
}

#[derive(Debug, PartialEq)]
pub enum RenameError {
    NoSuchUser,
    Taken,
    Invalid,
}

#[derive(Default)]
pub struct ProfileService {
    names: HashMap<u64, String>,
}

impl ProfileService {
    pub fn add(&mut self, id: u64, name: Username) {
        self.names.insert(id, name.0);
    }

    pub fn name_of(&self, id: u64) -> Option<&str> {
        self.names.get(&id).map(String::as_str)
    }

    pub fn rename(&mut self, id: u64, new_name: String) -> Result<(), RenameError> {
        if !self.names.contains_key(&id) {
            return Err(RenameError::NoSuchUser);
        }
        if self.names.values().any(|n| *n == new_name) {
            return Err(RenameError::Taken);
        }
        self.names.insert(id, new_name);
        Ok(())
    }
}

pub mod controller {
    use super::*;

    pub fn rename(service: &mut ProfileService, id: u64, raw: &str) -> Result<(), RenameError> {
        service.rename(id, raw.to_string())
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(raw: &str) -> Username {
        Username::try_from(raw).unwrap()
    }

    fn service() -> ProfileService {
        let mut service = ProfileService::default();
        service.add(1, name("alice"));
        service.add(2, name("bob"));
        service
    }

    #[test]
    fn service_accepts_only_parsed_usernames() {
        let mut service = service();
        assert_eq!(service.rename(1, name("  Carol ")), Ok(()), "проверенный ник принимается");
        assert_eq!(service.name_of(1), Some("carol"), "и сохраняется нормализованным");
    }

    #[test]
    fn service_still_guards_uniqueness_and_existence() {
        let mut service = service();
        assert_eq!(service.rename(1, name("BOB")), Err(RenameError::Taken), "чужой ник занят");
        assert_eq!(service.rename(9, name("dave")), Err(RenameError::NoSuchUser), "нет такого пользователя");
    }

    #[test]
    fn controller_rejects_garbage_before_it_reaches_the_service() {
        let mut service = service();
        for raw in ["  ADMIN !!! ", "ab", "1abc", ""] {
            assert_eq!(
                controller::rename(&mut service, 1, raw),
                Err(RenameError::Invalid),
                "{raw:?} не ник: ошибка должна возникнуть на границе"
            );
        }
        assert_eq!(service.name_of(1), Some("alice"), "ник после отказов не изменился");
    }

    #[test]
    fn controller_normalizes_valid_input() {
        let mut service = service();
        assert_eq!(controller::rename(&mut service, 1, "  Carol "), Ok(()), "годный ввод принимается");
        assert_eq!(service.name_of(1), Some("carol"), "нормализованный");
    }
}
