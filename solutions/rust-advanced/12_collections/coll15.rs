use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Eq)]
pub struct Post {
    pub id: u64,
    pub likes: u32,
    pub text: String,
}

impl PartialEq for Post {
    fn eq(&self, other: &Post) -> bool {
        self.id == other.id
    }
}

impl Hash for Post {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

fn main() {
    let p = Post { id: 1, likes: 0, text: "hi".to_string() };
    println!("{}", p == p.clone());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::hash::DefaultHasher;

    fn post(id: u64, likes: u32) -> Post {
        Post { id, likes, text: format!("post {id}") }
    }

    fn hash_of(p: &Post) -> u64 {
        let mut h = DefaultHasher::new();
        p.hash(&mut h);
        h.finish()
    }

    #[test]
    fn equal_posts_hash_equally() {
        assert_eq!(post(5, 1), post(5, 900));
        assert_eq!(hash_of(&post(5, 1)), hash_of(&post(5, 900)), "равные по id посты обязаны давать один хеш");
        assert_ne!(post(5, 1), post(6, 1));
    }

    #[test]
    fn a_set_keeps_one_copy_per_id() {
        let mut set = HashSet::new();
        for id in 0..50 {
            set.insert(post(id, 0));
        }
        for id in 0..50 {
            assert!(!set.insert(post(id, 7)), "пост {id} уже во множестве, второй раз он вставиться не должен");
        }
        assert_eq!(set.len(), 50);
    }

    #[test]
    fn lookup_by_a_stub_with_the_same_id_works() {
        let mut by_post: HashMap<Post, &str> = HashMap::new();
        for id in 0..50 {
            by_post.insert(post(id, id as u32), "seen");
        }
        for id in 0..50 {
            let probe = Post { id, likes: 0, text: String::new() };
            assert_eq!(by_post.get(&probe), Some(&"seen"), "пост {id} не найден по пустышке с тем же id");
        }
    }
}
