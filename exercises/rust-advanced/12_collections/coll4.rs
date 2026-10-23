// «Общие друзья» и «возможно, знакомы» это операции над множествами. Друзья хранятся в `HashSet<u64>`,
// пересечение и разность уже есть в стандартной библиотеке, но они возвращают ленивые итераторы по ссылкам.
// TODO: Допиши `mutual` (общие друзья, по возрастанию id) и `suggest`: друзья моих друзей, без меня и без тех,
// кто уже в друзьях. Чем больше общих друзей с человеком, тем он выше; при равенстве меньший id раньше.
use std::collections::{HashMap, HashSet};

pub type Graph = HashMap<u64, HashSet<u64>>;

pub fn mutual(a: &HashSet<u64>, b: &HashSet<u64>) -> Vec<u64> {
    todo!("пересечение, отсортированное по возрастанию")
}

pub fn suggest(me: u64, graph: &Graph) -> Vec<(u64, usize)> {
    todo!("кандидаты и число общих друзей, лучшие первыми")
}

fn main() {
    let graph = Graph::new();
    println!("{:?}", suggest(1, &graph));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(ids: &[u64]) -> HashSet<u64> {
        ids.iter().copied().collect()
    }

    fn graph() -> Graph {
        let mut g = Graph::new();
        g.insert(1, set(&[2, 3, 4]));
        g.insert(2, set(&[1, 3, 5, 6]));
        g.insert(3, set(&[1, 2, 5, 7]));
        g.insert(4, set(&[1, 5]));
        g.insert(5, set(&[2, 3, 4]));
        g
    }

    #[test]
    fn mutual_friends_are_sorted() {
        assert_eq!(mutual(&set(&[9, 3, 5, 1]), &set(&[5, 1, 8, 9])), vec![1, 5, 9]);
        assert!(mutual(&set(&[1]), &set(&[2])).is_empty());
    }

    #[test]
    fn suggestions_skip_me_and_current_friends() {
        let ids: Vec<u64> = suggest(1, &graph()).into_iter().map(|(id, _)| id).collect();
        assert!(!ids.contains(&1), "себя предлагать нельзя");
        for friend in [2, 3, 4] {
            assert!(!ids.contains(&friend), "{friend} уже друг, предлагать его не надо");
        }
    }

    #[test]
    fn more_shared_friends_means_higher_rank() {
        assert_eq!(suggest(1, &graph()), vec![(5, 3), (6, 1), (7, 1)], "5 общий у троих друзей, 6 и 7 у одного, при равенстве меньший id");
    }

    #[test]
    fn a_user_without_friends_gets_nothing() {
        assert!(suggest(42, &graph()).is_empty());
        let mut g = Graph::new();
        g.insert(1, HashSet::new());
        assert!(suggest(1, &g).is_empty());
    }
}
