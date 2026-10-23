#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    pub id: u64,
    pub score: f64,
}

pub fn rank(posts: &mut [Scored]) {
    posts.sort_by(|a, b| match (a.score.is_nan(), b.score.is_nan()) {
        (true, true) => std::cmp::Ordering::Equal,
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        (false, false) => b.score.total_cmp(&a.score),
    });
}

pub fn best(posts: &[Scored]) -> Option<&Scored> {
    posts.iter().filter(|p| !p.score.is_nan()).max_by(|a, b| a.score.total_cmp(&b.score))
}

fn main() {
    let mut posts = vec![Scored { id: 1, score: 0.5 }];
    rank(&mut posts);
    println!("{:?}", best(&posts));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(id: u64, score: f64) -> Scored {
        Scored { id, score }
    }

    fn ids(posts: &[Scored]) -> Vec<u64> {
        posts.iter().map(|p| p.id).collect()
    }

    #[test]
    fn plain_numbers_go_in_descending_order() {
        let mut posts = vec![s(1, 0.1), s(2, 0.9), s(3, 0.5)];
        rank(&mut posts);
        assert_eq!(ids(&posts), vec![2, 3, 1]);
    }

    #[test]
    fn nan_does_not_panic_and_goes_last() {
        let mut posts = vec![s(1, f64::NAN), s(2, 0.3), s(3, f64::NAN), s(4, 0.8)];
        rank(&mut posts);
        assert_eq!(ids(&posts), vec![4, 2, 1, 3], "NaN в конец, настоящие числа по убыванию, NaN между собой в прежнем порядке");
    }

    #[test]
    fn equal_scores_keep_their_order() {
        let mut posts: Vec<Scored> = (1..=40).map(|id| s(id, (id % 3) as f64)).collect();
        rank(&mut posts);
        let twos: Vec<u64> = posts.iter().filter(|p| p.score == 2.0).map(|p| p.id).collect();
        let mut sorted = twos.clone();
        sorted.sort_unstable();
        assert_eq!(twos, sorted, "посты с равным рейтингом должны остаться в исходном порядке");
        assert_eq!(posts[0].score, 2.0);
    }

    #[test]
    fn best_ignores_nan() {
        let posts = vec![s(1, f64::NAN), s(2, 0.3), s(3, 0.8), s(4, f64::NAN)];
        assert_eq!(best(&posts).map(|p| p.id), Some(3));
        assert_eq!(best(&[s(9, f64::NAN)]), None, "среди одних NaN лучшего нет");
        assert_eq!(best(&[]), None);
    }
}
