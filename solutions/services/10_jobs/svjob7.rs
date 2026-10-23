fn main() {}

pub struct Periodic {
    every: u64,
    next: u64,
}

fn spread(instance: &str, every: u64) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in instance.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash % every
}

impl Periodic {
    pub fn new(instance: &str, every: u64, now: u64) -> Periodic {
        Periodic { every, next: now + spread(instance, every) }
    }

    pub fn next_at(&self) -> u64 {
        self.next
    }

    pub fn due(&mut self, now: u64) -> Vec<u64> {
        if self.next > now {
            return Vec::new();
        }
        let fired = vec![self.next];
        let missed = (now - self.next) / self.every + 1;
        self.next += missed * self.every;
        fired
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const MINUTE: u64 = 60_000;

    #[test]
    fn on_time_it_fires_once_per_period() {
        let mut job = Periodic::new("auth-1", MINUTE, 0);
        let first = job.next_at();
        assert_eq!(job.due(first - 1), Vec::<u64>::new(), "до срока ничего");
        assert_eq!(job.due(first).len(), 1, "в срок ровно одно срабатывание");
        assert!(job.due(first).is_empty(), "тот же момент второй раз ничего не даёт");
        assert_eq!(job.next_at(), first + MINUTE, "следующее через период");
        assert_eq!(job.due(first + MINUTE).len(), 1, "и так каждую минуту");
    }

    #[test]
    fn after_a_long_pause_it_fires_once_not_a_batch() {
        let mut job = Periodic::new("auth-1", MINUTE, 0);
        let first = job.next_at();
        let late = first + 60 * MINUTE + 17;
        let fired = job.due(late);
        assert_eq!(fired.len(), 1, "пропущено 60 тиков, а чистить нужно один раз, не {} подряд", fired.len());
        assert!(job.due(late).is_empty(), "сразу после этого повторного срабатывания нет");
        assert!(job.next_at() > late, "следующий срок в будущем: {} против {late}", job.next_at());
        assert!(job.next_at() <= late + MINUTE, "и не дальше одного периода");
        assert_eq!((job.next_at() - first) % MINUTE, 0, "следующий срок остаётся на прежней сетке периода, а не плывёт от момента пробуждения");
    }

    #[test]
    fn a_wake_up_exactly_on_a_grid_point_does_not_double_fire() {
        let mut job = Periodic::new("auth-1", MINUTE, 0);
        let first = job.next_at();
        assert_eq!(job.due(first + 3 * MINUTE).len(), 1, "проснулись ровно на границе: одно срабатывание");
        assert_eq!(job.next_at(), first + 4 * MINUTE, "следующее строго после");
    }

    #[test]
    fn instances_start_at_different_moments() {
        let starts: Vec<u64> = (0..200).map(|i| Periodic::new(&format!("auth-{i}"), MINUTE, 1_000_000).next_at()).collect();
        let distinct: BTreeSet<u64> = starts.iter().copied().collect();
        assert!(distinct.len() >= 150, "двести экземпляров, а разных моментов старта только {}: все стартуют в одну секунду", distinct.len());
        assert!(starts.iter().all(|s| (1_000_000..1_000_000 + MINUTE).contains(s)), "первое срабатывание в пределах первого периода");
    }

    #[test]
    fn the_same_instance_always_starts_at_the_same_offset() {
        let a = Periodic::new("auth-7", MINUTE, 500).next_at() - 500;
        let b = Periodic::new("auth-7", MINUTE, 9_000_000).next_at() - 9_000_000;
        assert_eq!(a, b, "сдвиг зависит от имени экземпляра, а не от случая: после перезапуска он тот же");
    }
}
