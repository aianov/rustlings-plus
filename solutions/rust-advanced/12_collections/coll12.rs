use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub name: String,
    pub followers: u32,
    pub verified: bool,
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .verified
            .cmp(&self.verified)
            .then_with(|| other.followers.cmp(&self.followers))
            .then_with(|| self.name.cmp(&other.name))
    }
}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn main() {
    let c = Candidate { name: "ira".to_string(), followers: 10, verified: false };
    println!("{c:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn c(name: &str, followers: u32, verified: bool) -> Candidate {
        Candidate { name: name.to_string(), followers, verified }
    }

    fn sample() -> Vec<Candidate> {
        vec![
            c("zoya", 900, false),
            c("anna", 900, false),
            c("boris", 5, true),
            c("clara", 70, true),
            c("dmitri", 70, true),
            c("egor", 1000, false),
        ]
    }

    #[test]
    fn sorting_puts_the_best_first() {
        let mut list = sample();
        list.sort();
        let names: Vec<&str> = list.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["clara", "dmitri", "boris", "egor", "anna", "zoya"], "сначала подтверждённые, потом подписчики по убыванию, потом имя");
    }

    #[test]
    fn the_smallest_is_the_best_and_sets_agree() {
        let list = sample();
        assert_eq!(list.iter().min().map(|c| c.name.as_str()), Some("clara"));
        let set: BTreeSet<Candidate> = list.into_iter().collect();
        assert_eq!(set.first().map(|c| c.name.as_str()), Some("clara"));
        assert_eq!(set.last().map(|c| c.name.as_str()), Some("zoya"));
    }

    #[test]
    fn order_is_consistent_with_equality() {
        let list = sample();
        for a in &list {
            for b in &list {
                assert_eq!(a.cmp(b) == Ordering::Equal, a == b, "{a:?} и {b:?}: cmp и == расходятся");
                assert_eq!(a.partial_cmp(b), Some(a.cmp(b)));
                assert_eq!(a.cmp(b), b.cmp(a).reverse(), "порядок должен быть антисимметричным");
            }
        }
        let twin = c("same", 1, false);
        assert_eq!(twin.cmp(&twin.clone()), Ordering::Equal);
    }
}
