use std::collections::{HashMap, HashSet};

pub type Graph = HashMap<u64, HashSet<u64>>;

pub fn mutual(a: &HashSet<u64>, b: &HashSet<u64>) -> Vec<u64> {
    let mut common: Vec<u64> = a.intersection(b).copied().collect();
    common.sort_unstable();
    common
}

pub fn suggest(me: u64, graph: &Graph) -> Vec<(u64, usize)> {
    let Some(friends) = graph.get(&me) else {
        return Vec::new();
    };
    let mut shared: HashMap<u64, usize> = HashMap::new();
    for friend in friends {
        let Some(of_friend) = graph.get(friend) else {
            continue;
        };
        for candidate in of_friend.difference(friends) {
            if *candidate != me {
                *shared.entry(*candidate).or_insert(0) += 1;
            }
        }
    }
    let mut ranked: Vec<(u64, usize)> = shared.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked
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
