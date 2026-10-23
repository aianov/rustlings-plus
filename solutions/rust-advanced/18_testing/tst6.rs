use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: u32,
    pub name: String,
}

pub trait UserStore {
    fn get(&self, id: u32) -> Option<User>;
    fn put(&mut self, user: User) -> Option<User>;
    fn remove(&mut self, id: u32) -> bool;
    fn all(&self) -> Vec<User>;
}

#[derive(Default)]
pub struct MemStore {
    users: BTreeMap<u32, User>,
}

impl MemStore {
    pub fn new() -> MemStore {
        MemStore::default()
    }
}

impl UserStore for MemStore {
    fn get(&self, id: u32) -> Option<User> {
        self.users.get(&id).cloned()
    }

    fn put(&mut self, user: User) -> Option<User> {
        self.users.insert(user.id, user)
    }

    fn remove(&mut self, id: u32) -> bool {
        self.users.remove(&id).is_some()
    }

    fn all(&self) -> Vec<User> {
        self.users.values().cloned().collect()
    }
}

#[derive(Debug, PartialEq)]
pub enum DirError {
    NotFound(u32),
    EmptyName,
}

pub struct Directory<S: UserStore> {
    store: S,
}

impl<S: UserStore> Directory<S> {
    pub fn new(store: S) -> Self {
        Directory { store }
    }

    pub fn rename(&mut self, id: u32, name: &str) -> Result<(), DirError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DirError::EmptyName);
        }
        let mut user = self.store.get(id).ok_or(DirError::NotFound(id))?;
        user.name = name.to_string();
        self.store.put(user);
        Ok(())
    }

    pub fn import(&mut self, users: Vec<User>) -> usize {
        users.into_iter().filter(|user| self.store.put(user.clone()).is_none()).count()
    }

    pub fn names(&self) -> Vec<String> {
        self.store.all().into_iter().map(|user| user.name).collect()
    }

    pub fn into_store(self) -> S {
        self.store
    }
}

fn main() {
    let mut dir = Directory::new(MemStore::new());
    dir.import(vec![User { id: 1, name: "ann".to_string() }]);
    println!("{:?}", dir.names());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(id: u32, name: &str) -> User {
        User { id, name: name.to_string() }
    }

    #[test]
    fn put_returns_the_previous_user() {
        let mut store = MemStore::new();
        assert_eq!(store.put(user(1, "ann")), None, "первая запись ничего не заменяет");
        assert_eq!(store.put(user(1, "anna")), Some(user(1, "ann")), "вторая возвращает старого пользователя");
        assert_eq!(store.get(1), Some(user(1, "anna")), "в хранилище остаётся новая запись");
    }

    #[test]
    fn get_returns_a_copy() {
        let mut store = MemStore::new();
        store.put(user(1, "ann"));
        let mut copy = store.get(1).expect("пользователь должен быть");
        copy.name = "changed".to_string();
        assert_eq!(store.get(1), Some(user(1, "ann")), "правка копии не должна менять хранилище");
        assert_eq!(store.get(2), None);
    }

    #[test]
    fn remove_tells_whether_anything_was_there() {
        let mut store = MemStore::new();
        store.put(user(5, "eve"));
        assert!(store.remove(5), "пользователь был, remove должен вернуть true");
        assert!(!store.remove(5), "второй раз удалять уже нечего");
        assert_eq!(store.get(5), None);
    }

    #[test]
    fn all_is_ordered_by_id() {
        let mut store = MemStore::new();
        for id in [30, 2, 17, 5, 11, 40, 8, 23, 1, 19, 3, 29] {
            store.put(user(id, "x"));
        }
        let ids: Vec<u32> = store.all().into_iter().map(|u| u.id).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted, "all() обязан отдавать пользователей по возрастанию id, как ORDER BY id");
        assert_eq!(ids.len(), 12);
    }

    #[test]
    fn directory_renames_through_the_store() {
        let mut dir = Directory::new(MemStore::new());
        assert_eq!(dir.import(vec![user(2, "bob"), user(1, "ann")]), 2);
        assert_eq!(dir.rename(1, "  anna "), Ok(()));
        assert_eq!(dir.names(), vec!["anna".to_string(), "bob".to_string()]);
    }

    #[test]
    fn failed_rename_changes_nothing() {
        let mut dir = Directory::new(MemStore::new());
        dir.import(vec![user(1, "ann")]);
        assert_eq!(dir.rename(9, "zoe"), Err(DirError::NotFound(9)));
        assert_eq!(dir.rename(1, "   "), Err(DirError::EmptyName));
        assert_eq!(dir.names(), vec!["ann".to_string()], "после неудачных переименований имя прежнее");
    }

    #[test]
    fn import_counts_only_new_users() {
        let mut dir = Directory::new(MemStore::new());
        assert_eq!(dir.import(vec![user(1, "a"), user(2, "b")]), 2);
        assert_eq!(dir.import(vec![user(2, "bb"), user(3, "c")]), 1, "id 2 уже был: это замена, а не новый пользователь");
        let store = dir.into_store();
        assert_eq!(store.get(2), Some(user(2, "bb")), "при повторе побеждает последняя запись");
    }
}
