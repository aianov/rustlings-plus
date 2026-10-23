use std::marker::PhantomData;

pub struct Id<T> {
    raw: u64,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    pub fn new(raw: u64) -> Self {
        Id { raw, _marker: PhantomData }
    }

    pub fn raw(&self) -> u64 {
        self.raw
    }
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Id<T> {}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl<T> Eq for Id<T> {}

impl<T> std::hash::Hash for Id<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}

impl<T> std::fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Id({})", self.raw)
    }
}

pub struct Post {
    pub text: String,
}

fn main() {
    let id: Id<Post> = Id::new(1);
    println!("{id:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn id_of_a_plain_struct_is_copy() {
        let id: Id<Post> = Id::new(3);
        let a = id;
        let b = id;
        assert_eq!(a.raw(), b.raw(), "после копирования исходный id остаётся доступным");
        #[allow(clippy::clone_on_copy)]
        let c = id.clone();
        assert_eq!(c, id);
    }

    #[test]
    fn ids_compare_by_number_only() {
        assert_eq!(Id::<Post>::new(1), Id::<Post>::new(1));
        assert_ne!(Id::<Post>::new(1), Id::<Post>::new(2));
    }

    #[test]
    fn id_is_a_hash_key() {
        let mut likes: HashMap<Id<Post>, u32> = HashMap::new();
        *likes.entry(Id::new(7)).or_insert(0) += 1;
        *likes.entry(Id::new(7)).or_insert(0) += 1;
        assert_eq!(likes.get(&Id::new(7)), Some(&2), "тот же номер, тот же ключ");
        assert_eq!(likes.len(), 1);
    }

    #[test]
    fn debug_shows_only_the_number() {
        assert_eq!(format!("{:?}", Id::<Post>::new(7)), "Id(7)");
    }

    #[test]
    fn works_with_non_static_marker() {
        let id: Id<&str> = Id::new(2);
        let copy = id;
        assert_eq!(id, copy);
    }
}
