use std::sync::atomic::{AtomicU64, Ordering};

fn main() {}

#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub age: u8,
    pub verified: bool,
    pub banned: bool,
    pub followers: Vec<u64>,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1000);

#[derive(Debug, Clone)]
pub struct UserBuilder {
    user: User,
}

impl UserBuilder {
    pub fn new() -> UserBuilder {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        UserBuilder {
            user: User { id, name: format!("User {id}"), email: format!("user{id}@test.local"), age: 30, verified: true, banned: false, followers: Vec::new() },
        }
    }

    pub fn id(mut self, id: u64) -> UserBuilder {
        self.user.id = id;
        self
    }

    pub fn name(mut self, name: &str) -> UserBuilder {
        self.user.name = name.to_string();
        self
    }

    pub fn email(mut self, email: &str) -> UserBuilder {
        self.user.email = email.to_string();
        self
    }

    pub fn age(mut self, age: u8) -> UserBuilder {
        self.user.age = age;
        self
    }

    pub fn unverified(mut self) -> UserBuilder {
        self.user.verified = false;
        self
    }

    pub fn banned(mut self) -> UserBuilder {
        self.user.banned = true;
        self
    }

    pub fn followers(mut self, count: usize) -> UserBuilder {
        self.user.followers = (0..count).map(|_| NEXT_ID.fetch_add(1, Ordering::SeqCst)).collect();
        self
    }

    pub fn build(self) -> User {
        self.user
    }
}

impl Default for UserBuilder {
    fn default() -> Self {
        UserBuilder::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn the_defaults_are_a_valid_ordinary_user() {
        let user = UserBuilder::new().build();
        assert!(!user.name.trim().is_empty(), "имя заполнено");
        assert!(user.email.contains('@') && !user.email.starts_with('@') && !user.email.ends_with('@'), "почта выглядит как почта: {:?}", user.email);
        assert!(user.age >= 18, "по умолчанию пользователь взрослый, иначе половина проверок возраста срабатывает сама: {}", user.age);
        assert!(user.verified && !user.banned, "по умолчанию подтверждён и не заблокирован: обычный человек, на нём не должны срабатывать особые случаи");
        assert!(user.followers.is_empty(), "подписчиков нет, пока тест их не попросил");
    }

    #[test]
    fn every_new_user_is_distinct() {
        let users: Vec<User> = (0..50).map(|_| UserBuilder::new().build()).collect();
        let ids: HashSet<u64> = users.iter().map(|u| u.id).collect();
        let emails: HashSet<&String> = users.iter().map(|u| &u.email).collect();
        assert_eq!(ids.len(), 50, "id не повторяются, иначе два пользователя в одной базе столкнутся");
        assert_eq!(emails.len(), 50, "почты не повторяются: уникальный индекс не должен ломать посторонние тесты");
    }

    #[test]
    fn defaults_are_unique_even_across_threads() {
        let handles: Vec<_> = (0..8).map(|_| std::thread::spawn(|| (0..100).map(|_| UserBuilder::new().build().id).collect::<Vec<_>>())).collect();
        let all: Vec<u64> = handles.into_iter().flat_map(|h| h.join().unwrap()).collect();
        let unique: HashSet<u64> = all.iter().copied().collect();
        assert_eq!(unique.len(), all.len(), "тесты идут параллельно: счётчик номеров должен быть общим и атомарным");
    }

    #[test]
    fn a_test_overrides_only_what_it_cares_about() {
        let user = UserBuilder::new().name("Zed").age(15).banned().unverified().build();
        assert_eq!((user.name.as_str(), user.age, user.banned, user.verified), ("Zed", 15, true, false), "заданное применилось");
        let plain = UserBuilder::new().name("Zed").build();
        assert_ne!(plain.email, "zed", "остальные поля остались умолчаниями");
        assert!(plain.email.contains('@'), "почта осталась правдоподобной, хоть имя поменяли: {:?}", plain.email);
        let fixed = UserBuilder::new().id(7).email("a@b.c").build();
        assert_eq!((fixed.id, fixed.email.as_str()), (7, "a@b.c"), "id и почту можно задать явно");
    }

    #[test]
    fn followers_are_distinct_ids_not_the_user_itself() {
        let user = UserBuilder::new().followers(5).build();
        let unique: HashSet<u64> = user.followers.iter().copied().collect();
        assert_eq!(user.followers.len(), 5, "запрошено пять подписчиков");
        assert_eq!(unique.len(), 5, "все разные");
        assert!(!unique.contains(&user.id), "сам на себя пользователь не подписан");
        let other = UserBuilder::new().followers(3).build();
        assert!(other.followers.iter().all(|f| !unique.contains(f)), "подписчики двух разных пользователей не пересекаются: это разные люди");
    }

    #[test]
    fn a_builder_can_be_reused_as_a_template() {
        let template = UserBuilder::new().age(40).banned();
        let a = template.clone().build();
        let b = template.build();
        assert_eq!((a.age, a.banned, b.age, b.banned), (40, true, 40, true), "шаблон копируется вместе с настройками");
        assert_eq!(a.id, b.id, "копия того же шаблона это тот же пользователь: id задан при создании шаблона, а не при сборке");
    }
}
