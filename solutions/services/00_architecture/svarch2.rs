pub mod repository {
    use std::collections::HashSet;

    #[derive(Default)]
    pub struct UserStore {
        names: HashSet<String>,
    }

    impl UserStore {
        pub fn contains(&self, name: &str) -> bool {
            self.names.contains(name)
        }

        pub fn insert(&mut self, name: String) {
            self.names.insert(name);
        }

        pub fn count(&self) -> usize {
            self.names.len()
        }
    }
}

pub mod service {
    use super::repository::UserStore;

    #[derive(Debug, PartialEq)]
    pub enum RegisterError {
        Empty,
        Taken,
    }

    #[derive(Default)]
    pub struct UserService {
        store: UserStore,
    }

    impl UserService {
        pub fn register(&mut self, raw: &str) -> Result<String, RegisterError> {
            let name = raw.trim().to_lowercase();
            if name.is_empty() {
                return Err(RegisterError::Empty);
            }
            if self.store.contains(&name) {
                return Err(RegisterError::Taken);
            }
            self.store.insert(name.clone());
            Ok(name)
        }

        pub fn user_count(&self) -> usize {
            self.store.count()
        }
    }
}

pub mod controller {
    use super::service::{RegisterError, UserService};

    pub fn handle(service: &mut UserService, line: &str) -> String {
        let Some(raw) = line.strip_prefix("register ") else {
            return "400 unknown command".to_string();
        };
        match service.register(raw) {
            Ok(name) => format!("201 {name}"),
            Err(RegisterError::Empty) => "422 empty name".to_string(),
            Err(RegisterError::Taken) => "409 taken".to_string(),
        }
    }
}

fn main() {
    let mut service = service::UserService::default();
    println!("{}", controller::handle(&mut service, "register alice"));
}

#[cfg(test)]
mod tests {
    use super::controller;
    use super::service::{RegisterError, UserService};

    #[test]
    fn service_normalizes_and_stores() {
        let mut service = UserService::default();
        assert_eq!(
            service.register("  Alice "),
            Ok("alice".to_string()),
            "service должен вернуть нормализованный ник, а не текст ответа"
        );
        assert_eq!(service.user_count(), 1, "ник должен попасть в репозиторий");
    }

    #[test]
    fn service_rejects_empty_and_taken() {
        let mut service = UserService::default();
        assert_eq!(service.register("   "), Err(RegisterError::Empty), "пустой ник отклоняется");
        assert_eq!(service.user_count(), 0, "отклонённый ник нельзя сохранять");
        service.register("bob").unwrap();
        assert_eq!(
            service.register("BOB"),
            Err(RegisterError::Taken),
            "ник сравнивается после нормализации: BOB это тот же bob"
        );
        assert_eq!(service.user_count(), 1, "дубликат не должен попасть в репозиторий");
    }

    #[test]
    fn controller_translates_results_to_text() {
        let mut service = UserService::default();
        assert_eq!(controller::handle(&mut service, "register Alice"), "201 alice", "успех: 201 и ник");
        assert_eq!(controller::handle(&mut service, "register alice"), "409 taken", "дубликат: 409");
        assert_eq!(controller::handle(&mut service, "register  "), "422 empty name", "пустой ник: 422");
        assert_eq!(
            controller::handle(&mut service, "ping"),
            "400 unknown command",
            "незнакомая команда разбирается в контроллере"
        );
    }
}
