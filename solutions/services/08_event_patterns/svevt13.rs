use std::collections::HashMap;

fn main() {}

#[derive(Debug, Clone)]
pub struct ProfileEvent {
    pub user_id: String,
    pub version: u64,
    pub occurred_at: u64,
    pub name: String,
}

#[derive(Default)]
pub struct Profiles {
    names: HashMap<String, (u64, String)>,
}

impl Profiles {
    pub fn new() -> Profiles {
        Profiles::default()
    }

    pub fn name(&self, user_id: &str) -> Option<&str> {
        self.names.get(user_id).map(|(_, name)| name.as_str())
    }

    pub fn apply(&mut self, event: &ProfileEvent) -> bool {
        if let Some((current, _)) = self.names.get(&event.user_id)
            && *current >= event.version
        {
            return false;
        }
        self.names.insert(event.user_id.clone(), (event.version, event.name.clone()));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rename(user: &str, version: u64, occurred_at: u64, name: &str) -> ProfileEvent {
        ProfileEvent { user_id: user.into(), version, occurred_at, name: name.into() }
    }

    #[test]
    fn in_order_events_all_apply() {
        let mut profiles = Profiles::new();
        assert!(profiles.apply(&rename("u1", 1, 10, "Ann")), "первое событие применено");
        assert!(profiles.apply(&rename("u1", 2, 20, "Anna")), "второе применено");
        assert_eq!(profiles.name("u1"), Some("Anna"), "имя последнее");
    }

    #[test]
    fn an_older_event_arriving_late_does_not_overwrite_the_newer_one() {
        let mut profiles = Profiles::new();
        profiles.apply(&rename("u1", 1, 10, "Ann"));
        profiles.apply(&rename("u1", 3, 30, "Anna"));
        assert!(!profiles.apply(&rename("u1", 2, 20, "Anya")), "версия 2 опоздала, применять её нельзя");
        assert_eq!(profiles.name("u1"), Some("Anna"), "осталось имя из версии 3");
    }

    #[test]
    fn a_repeat_changes_nothing() {
        let mut profiles = Profiles::new();
        assert!(profiles.apply(&rename("u1", 2, 20, "Anna")));
        assert!(!profiles.apply(&rename("u1", 2, 20, "Anna")), "та же версия: повтор");
        assert!(!profiles.apply(&rename("u1", 2, 25, "Other")), "та же версия с другим содержимым тоже не применяется: версия в агрегате одна");
        assert_eq!(profiles.name("u1"), Some("Anna"));
    }

    #[test]
    fn the_version_decides_not_the_clock() {
        let mut profiles = Profiles::new();
        profiles.apply(&rename("u1", 1, 1_000, "Ann"));
        assert!(profiles.apply(&rename("u1", 2, 900, "Anna")), "у версии 2 время на сервере отстало, но она новее");
        assert_eq!(profiles.name("u1"), Some("Anna"), "по часам старое, по версии новое: верим версии");
        assert!(!profiles.apply(&rename("u1", 1, 5_000, "Ann again")), "версия 1 с огромным временем всё равно старая");
    }

    #[test]
    fn gaps_are_fine_for_snapshots_and_users_are_independent() {
        let mut profiles = Profiles::new();
        assert!(profiles.apply(&rename("u1", 5, 50, "Zed")), "первое событие может быть с любой версией: потерянные промежуточные не нужны, имя целиком");
        assert!(profiles.apply(&rename("u2", 1, 10, "Bob")), "другой пользователь с версии 1");
        assert_eq!((profiles.name("u1"), profiles.name("u2")), (Some("Zed"), Some("Bob")), "оба на месте");
        assert_eq!(profiles.name("nobody"), None, "неизвестного нет");
    }
}
