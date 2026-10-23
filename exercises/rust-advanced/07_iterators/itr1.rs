// Табло с тиками: счётчик выдаёт `start`, `start + step`, `start + 2 * step` и так далее, пока значение
// не станет больше `end` (сам `end` включается, если попадает на шаг). Трейт `Iterator` просит один метод:
// `next`, который отдаёт `Some(следующее)` или `None`, когда всё кончилось. После `None` счётчик больше
// ничего не выдаёт. Если очередное значение не помещается в `u32`, это тоже конец, а не паника и не
// бесконечный цикл по кругу. Нулевой шаг зациклил бы счётчик, поэтому `new` отвергает его паникой
// с сообщением `step must be positive`.
// TODO: Реализуй `Iterator` для `Ticks`.

pub struct Ticks {
    next: Option<u32>,
    end: u32,
    step: u32,
}

impl Ticks {
    pub fn new(start: u32, end: u32, step: u32) -> Ticks {
        assert!(step > 0, "step must be positive");
        Ticks { next: Some(start), end, step }
    }
}

impl Iterator for Ticks {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        todo!("отдать текущее значение и подготовить следующее")
    }
}

fn main() {
    println!("{:?}", Ticks::new(1, 10, 3).collect::<Vec<_>>());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_with_a_step() {
        assert_eq!(Ticks::new(1, 10, 3).collect::<Vec<_>>(), vec![1, 4, 7, 10], "конец включается, если попал на шаг");
        assert_eq!(Ticks::new(0, 9, 4).collect::<Vec<_>>(), vec![0, 4, 8]);
    }

    #[test]
    fn empty_when_start_is_past_the_end() {
        assert_eq!(Ticks::new(5, 4, 1).count(), 0);
    }

    #[test]
    fn stops_instead_of_overflowing() {
        let got: Vec<u32> = Ticks::new(u32::MAX - 4, u32::MAX, 3).collect();
        assert_eq!(got, vec![u32::MAX - 4, u32::MAX - 1], "следующее значение не помещается в u32: это конец");
        assert_eq!(Ticks::new(u32::MAX, u32::MAX, 1).collect::<Vec<_>>(), vec![u32::MAX]);
    }

    #[test]
    fn stays_finished() {
        let mut ticks = Ticks::new(1, 2, 1);
        assert_eq!(ticks.next(), Some(1));
        assert_eq!(ticks.next(), Some(2));
        assert_eq!(ticks.next(), None);
        assert_eq!(ticks.next(), None, "после None остаётся None");
    }

    #[test]
    fn std_adapters_work_on_it() {
        assert_eq!(Ticks::new(1, 100, 1).take(3).sum::<u32>(), 6);
        let zipped: Vec<(u32, char)> = Ticks::new(10, 100, 10).zip("abc".chars()).collect();
        assert_eq!(zipped, vec![(10, 'a'), (20, 'b'), (30, 'c')]);
    }

    #[test]
    #[should_panic(expected = "step must be positive")]
    fn zero_step_is_rejected() {
        let _ = Ticks::new(0, 5, 0);
    }
}
