// Шлюз ходит в три экземпляра post-service по кругу: первый, второй, третий, первый... Один экземпляр упал. Круг по-прежнему честно доходит до него
// каждый третий раз, и треть запросов получает ошибку, хотя два других экземпляра здоровы и скучают. Балансировщик должен замечать больные экземпляры, переставать
// слать им запросы и осторожно проверять, не выздоровели ли они.
// Цель: после трёх неудач подряд экземпляр выводится из ротации на `cooldown`; потом ему отправляется один пробный запрос; успех возвращает его в круг, неудача отправляет
// обратно на новую паузу. Остальные экземпляры при этом делят запросы по кругу поровну. Время передаётся аргументом.


use std::time::{Duration, Instant};

fn main() {}

const THRESHOLD: u32 = 3;

#[derive(Default, Clone)]
struct Inst {
    failures: u32,
    down_until: Option<Instant>,
    probing: bool,
}

pub struct Balancer {
    instances: Vec<Inst>,
    next: usize,
    cooldown: Duration,
}

impl Balancer {
    pub fn new(count: usize, cooldown: Duration) -> Balancer {
        Balancer { instances: vec![Inst::default(); count], next: 0, cooldown }
    }

    /// Выбирает экземпляр для запроса или `None`, если выбрать нельзя.
    pub fn pick(&mut self, now: Instant) -> Option<usize> {
        let _ = (now, THRESHOLD, self.cooldown);
        let n = self.instances.len();
        let i = self.next % n;
        self.next += 1;
        Some(i)
    }

    /// Сообщает исход запроса к экземпляру `i`.
    pub fn report(&mut self, i: usize, ok: bool, now: Instant) {
        let _ = (i, ok, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: fn(u64) -> Duration = Duration::from_secs;

    fn picks(b: &mut Balancer, n: usize, now: Instant) -> Vec<usize> {
        (0..n).filter_map(|_| b.pick(now)).collect()
    }

    fn fail_n(b: &mut Balancer, i: usize, n: u32, now: Instant) {
        for _ in 0..n {
            b.report(i, false, now);
        }
    }

    fn counts(p: &[usize], n: usize) -> Vec<usize> {
        (0..n).map(|i| p.iter().filter(|x| **x == i).count()).collect()
    }

    #[test]
    fn healthy_instances_share_requests_evenly() {
        let t0 = Instant::now();
        let mut b = Balancer::new(3, S(5));
        assert_eq!(counts(&picks(&mut b, 90, t0), 3), vec![30, 30, 30]);
    }

    #[test]
    fn a_sick_instance_is_taken_out_of_rotation() {
        let t0 = Instant::now();
        let mut b = Balancer::new(3, S(5));
        fail_n(&mut b, 1, 3, t0);
        let p = picks(&mut b, 90, t0 + S(1));
        assert_eq!(counts(&p, 3), vec![45, 0, 45], "больному экземпляру ушло {} запросов, остальные должны делить поровну", counts(&p, 3)[1]);
    }

    #[test]
    fn a_success_resets_the_failure_streak() {
        let t0 = Instant::now();
        let mut b = Balancer::new(2, S(5));
        fail_n(&mut b, 0, 2, t0);
        b.report(0, true, t0);
        fail_n(&mut b, 0, 2, t0);
        assert_eq!(counts(&picks(&mut b, 20, t0), 2), vec![10, 10], "две неудачи, успех, две неудачи: подряд их не три, экземпляр здоров");
    }

    #[test]
    fn after_the_cooldown_exactly_one_probe_goes_through() {
        let t0 = Instant::now();
        let mut b = Balancer::new(3, S(5));
        fail_n(&mut b, 1, 3, t0);
        let later = t0 + S(6);
        let p = picks(&mut b, 12, later);
        assert_eq!(counts(&p, 3)[1], 1, "пробный запрос один, пока он не вернулся: {p:?}");
        b.report(1, true, later);
        assert_eq!(counts(&picks(&mut b, 30, later), 3), vec![10, 10, 10], "проба удалась, экземпляр вернулся в круг");
    }

    #[test]
    fn a_failed_probe_sends_it_back_for_another_cooldown() {
        let t0 = Instant::now();
        let mut b = Balancer::new(2, S(5));
        fail_n(&mut b, 0, 3, t0);
        let later = t0 + S(6);
        assert!(picks(&mut b, 4, later).contains(&0));
        b.report(0, false, later);
        assert_eq!(counts(&picks(&mut b, 10, later + S(1)), 2), vec![0, 10], "проба провалилась: снова пауза с этого момента");
        assert!(picks(&mut b, 4, later + S(6)).contains(&0));
    }

    #[test]
    fn when_everybody_is_down_there_is_no_choice() {
        let t0 = Instant::now();
        let mut b = Balancer::new(2, S(5));
        fail_n(&mut b, 0, 3, t0);
        fail_n(&mut b, 1, 3, t0);
        assert_eq!(b.pick(t0 + S(1)), None, "все больны и пауза не прошла: честнее отказать сразу, чем бить в упавшее");
        assert!(b.pick(t0 + S(6)).is_some(), "после паузы можно пробовать");
    }
}
