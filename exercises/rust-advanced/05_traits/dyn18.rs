// Контекст запроса должен переносить произвольные данные от middleware к обработчику: идентификатор
// запроса, текущего пользователя, соединение с базой. Заранее неизвестно, какие типы туда положат,
// поэтому нужна типизированная карта: ключом служит сам тип, значением его экземпляр. Для этого
// пригодны `TypeId` и `dyn Any`: значение кладётся как `Box<dyn Any>`, а при чтении возвращается
// обратно в конкретный тип через `downcast_ref`.
// В карте не бывает двух значений одного типа: второй `insert` того же типа заменяет первое и
// возвращает старое.
// TODO: Реализуй `Extensions`.

use std::any::{Any, TypeId};
use std::collections::HashMap;

#[derive(Default)]
pub struct Extensions {
    map: HashMap<TypeId, Box<dyn Any>>,
}

impl Extensions {
    pub fn new() -> Extensions {
        Extensions::default()
    }

    pub fn insert<T: Any>(&mut self, value: T) -> Option<T> {
        todo!("положить значение, вернуть прежнее того же типа")
    }

    pub fn get<T: Any>(&self) -> Option<&T> {
        todo!()
    }

    pub fn get_mut<T: Any>(&mut self) -> Option<&mut T> {
        todo!()
    }

    pub fn remove<T: Any>(&mut self) -> Option<T> {
        todo!()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

fn main() {
    let mut ext = Extensions::new();
    ext.insert(42u32);
    println!("{:?}", ext.get::<u32>());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct RequestId(u64);

    #[derive(Debug, PartialEq)]
    struct User(String);

    #[test]
    fn different_types_do_not_collide() {
        let mut ext = Extensions::new();
        ext.insert(RequestId(7));
        ext.insert(User("ann".into()));
        ext.insert(7u64);
        assert_eq!(ext.get::<RequestId>(), Some(&RequestId(7)));
        assert_eq!(ext.get::<User>(), Some(&User("ann".into())));
        assert_eq!(ext.get::<u64>(), Some(&7), "u64 и RequestId это разные ключи");
        assert_eq!(ext.len(), 3);
    }

    #[test]
    fn missing_type_is_none() {
        let mut ext = Extensions::new();
        assert!(ext.is_empty());
        ext.insert(1i32);
        assert_eq!(ext.get::<i64>(), None, "i64 не клали");
        assert_eq!(ext.get::<String>(), None);
    }

    #[test]
    fn insert_replaces_and_returns_old() {
        let mut ext = Extensions::new();
        assert_eq!(ext.insert(User("a".into())), None);
        assert_eq!(ext.insert(User("b".into())), Some(User("a".into())), "возвращается прежнее значение того же типа");
        assert_eq!(ext.get::<User>(), Some(&User("b".into())));
        assert_eq!(ext.len(), 1);
    }

    #[test]
    fn mutation_and_removal() {
        let mut ext = Extensions::new();
        ext.insert(vec![1, 2]);
        ext.get_mut::<Vec<i32>>().expect("вектор есть").push(3);
        assert_eq!(ext.get::<Vec<i32>>(), Some(&vec![1, 2, 3]));
        assert_eq!(ext.remove::<Vec<i32>>(), Some(vec![1, 2, 3]));
        assert_eq!(ext.remove::<Vec<i32>>(), None);
        assert!(ext.is_empty());
    }
}
