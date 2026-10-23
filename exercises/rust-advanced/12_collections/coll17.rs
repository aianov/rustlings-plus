// Три сортировки в одном сервисе, и у каждой своё требование. Страница профиля: больше лайков выше, при
// равенстве новее выше, потом по id. Сводка по лайкам: порядок только по лайкам, но равные не должны
// перемешиваться, иначе после обновления страницы посты «прыгают». Список ников по нижнему регистру:
// функция ключа дорогая (ниже она считает свои вызовы), и вызывать её на каждое сравнение расточительно.
// TODO: Допиши `profile_order`, `by_likes_stable` и `sort_names`.
use std::cmp::Reverse;

#[derive(Debug, Clone, PartialEq)]
pub struct Post {
    pub id: u64,
    pub likes: u32,
    pub created: u64,
}

pub fn profile_order(posts: &mut [Post]) {
    todo!("лайки по убыванию, затем новее, затем id")
}

pub fn by_likes_stable(posts: &mut [Post]) {
    todo!("только по лайкам по убыванию; равные в прежнем порядке")
}

pub fn sort_names(names: &mut [String], key: impl Fn(&str) -> String) {
    todo!("по key(имя), вычисляя ключ один раз на элемент")
}

fn main() {
    let mut posts = vec![Post { id: 1, likes: 3, created: 10 }];
    profile_order(&mut posts);
    println!("{posts:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn post(id: u64, likes: u32, created: u64) -> Post {
        Post { id, likes, created }
    }

    #[test]
    fn profile_has_three_criteria() {
        let mut posts = vec![post(1, 5, 100), post(2, 9, 50), post(3, 5, 300), post(4, 5, 300), post(5, 0, 999)];
        profile_order(&mut posts);
        let ids: Vec<u64> = posts.iter().map(|p| p.id).collect();
        assert_eq!(ids, vec![2, 3, 4, 1, 5], "9 лайков первый; среди пяти лайков новее выше, при одинаковом времени меньший id");
    }

    #[test]
    fn equal_likes_never_shuffle() {
        let mut posts: Vec<Post> = (0..600).map(|i| post(i, (i % 3) as u32, 0)).collect();
        by_likes_stable(&mut posts);
        assert!(posts[0].likes >= posts[599].likes);
        for likes in 0..3 {
            let ids: Vec<u64> = posts.iter().filter(|p| p.likes == likes).map(|p| p.id).collect();
            let mut sorted = ids.clone();
            sorted.sort_unstable();
            assert_eq!(ids, sorted, "посты с {likes} лайками переставились: сортировка должна быть устойчивой");
        }
        assert_eq!(posts.iter().filter(|p| p.likes == 2).count(), 200);
    }

    #[test]
    fn the_expensive_key_is_computed_once_per_item() {
        let calls = Cell::new(0);
        let mut names: Vec<String> = ["Zed", "alice", "Bob", "carol", "Dave", "eve", "Frank"].iter().map(|s| s.to_string()).collect();
        let total = names.len();
        sort_names(&mut names, |n| {
            calls.set(calls.get() + 1);
            n.to_lowercase()
        });
        assert_eq!(names, vec!["alice", "Bob", "carol", "Dave", "eve", "Frank", "Zed"]);
        assert_eq!(calls.get(), total, "ключ должен считаться по разу на имя, а не на каждое сравнение");
    }
}
