// Таблица рекордов хранит очки любого типа: целые, вещественные, свои структуры. Автор поставил границы
// прямо на структуру «на всякий случай», и теперь `Scoreboard<f64>` не создать: у `f64` нет полного
// порядка (`Ord`), хотя завести таблицу, добавлять в неё очки и считать их ничто не мешает. Искать лучшее
// очко можно там, где значения сравнимы, а `top` ещё и возвращает копии.
// TODO: Сделай так, чтобы каждый метод требовал ровно то, что ему нужно, и ни капли больше.
pub struct Scoreboard<T: Ord + Clone> {
    items: Vec<T>,
}

impl<T: Ord + Clone> Scoreboard<T> {
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

    pub fn best(&self) -> Option<&T> {
        self.items.iter().max()
    }

    pub fn top(&self, n: usize) -> Vec<T> {
        let mut sorted = self.items.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        sorted.truncate(n);
        sorted
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
