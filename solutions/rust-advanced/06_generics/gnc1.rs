pub struct Scoreboard<T> {
    items: Vec<T>,
}

impl<T> Scoreboard<T> {
    pub fn new() -> Self {
        Scoreboard { items: Vec::new() }
    }

    pub fn push(&mut self, score: T) {
        self.items.push(score);
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<T: Ord> Scoreboard<T> {
    pub fn best(&self) -> Option<&T> {
        self.items.iter().max()
    }
}

impl<T: Ord + Clone> Scoreboard<T> {
    pub fn top(&self, n: usize) -> Vec<T> {
        let mut sorted = self.items.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        sorted.truncate(n);
        sorted
    }
}

impl<T> Default for Scoreboard<T> {
    fn default() -> Self {
        Scoreboard::new()
    }
}

fn main() {
    let mut board = Scoreboard::new();
    board.push(40);
    board.push(75);
    println!("{:?}", board.best());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
    struct Ranked(u32);

    struct Opaque;

    #[test]
    fn floats_and_opaque_values_can_be_stored() {
        let mut floats = Scoreboard::new();
        floats.push(1.5);
        floats.push(f64::NAN);
        assert_eq!(floats.len(), 2, "f64 нельзя упорядочить, но хранить и считать его можно");
        let mut opaque = Scoreboard::new();
        assert!(opaque.is_empty());
        opaque.push(Opaque);
        assert_eq!(opaque.len(), 1, "тип без всяких свойств тоже годится для хранения");
    }

    #[test]
    fn best_needs_only_order_not_clone() {
        let mut board = Scoreboard::new();
        for score in [Ranked(10), Ranked(30), Ranked(20)] {
            board.push(score);
        }
        assert_eq!(board.best(), Some(&Ranked(30)), "best работает для Ord и не должен требовать Clone");
        assert_eq!(Scoreboard::<Ranked>::new().best(), None);
    }

    #[test]
    fn top_returns_sorted_copies() {
        let mut board = Scoreboard::new();
        for word in ["pear", "apple", "zebra", "mango"] {
            board.push(word.to_string());
        }
        assert_eq!(board.top(2), vec!["zebra".to_string(), "pear".to_string()], "от большего к меньшему");
        assert_eq!(board.top(10).len(), 4, "если очков меньше n, вернуть все");
        assert_eq!(board.len(), 4, "top не опустошает таблицу");
    }
}
