// Повтор по номеру попытки не знает, как дела у сервиса в целом. Если сервис болен, повторяют все клиенты сразу, и нагрузка от
// повторов складывается с обычной. Хорошее правило: на повторы уходит не больше определённой доли от обычного потока запросов.
// Цель: `RetryBudget`, который за каждый запрос выдаёт небольшой кредит и тратит кредит целиком на каждый повтор. Копить кредит
// безгранично нельзя: после долгой тишины болезнь сервиса не должна встретить гору накопленных повторов.


fn main() {}

pub struct RetryBudget {
    ratio: f64,
    reserve: f64,
    balance: f64,
}

impl RetryBudget {
    /// `ratio`: сколько повторов даёт один запрос (0.1 значит десятая доля).
    /// `reserve`: сколько повторов разрешено «с холода» и сколько кредита можно накопить максимум.
    pub fn new(ratio: f64, reserve: u32) -> RetryBudget {
        let _ = (ratio, reserve);
        todo!()
    }

    /// Учесть обычный запрос.
    pub fn on_request(&mut self) {
        todo!()
    }

    /// Можно ли сделать повтор прямо сейчас. Если да, кредит списан.
    pub fn try_retry(&mut self) -> bool {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cold_start_allows_only_the_reserve() {
        let mut b = RetryBudget::new(0.1, 10);
        let granted = (0..50).filter(|_| b.try_retry()).count();
        assert_eq!(granted, 10, "без единого запроса разрешено ровно столько повторов, сколько в резерве");
    }

    #[test]
    fn under_a_full_outage_retries_stay_near_the_ratio() {
        let mut b = RetryBudget::new(0.1, 10);
        let mut granted = 0;
        for _ in 0..1000 {
            b.on_request();
            if b.try_retry() {
                granted += 1;
            }
        }
        assert!(granted <= 111, "на 1000 запросов выдано {granted} повторов: больше десятой доли плюс резерв");
        assert!(granted >= 95, "выдано всего {granted} повторов: бюджет слишком жадный, десятая доля должна быть доступна");
    }

    #[test]
    fn credit_is_not_hoarded_during_quiet_times() {
        let mut b = RetryBudget::new(0.1, 10);
        for _ in 0..10_000 {
            b.on_request();
        }
        let burst = (0..100).filter(|_| b.try_retry()).count();
        assert!(burst <= 11, "после тихого времени сразу разрешено {burst} повторов подряд: кредит копится без потолка");
    }

    #[test]
    fn credit_returns_with_traffic() {
        let mut b = RetryBudget::new(0.1, 3);
        while b.try_retry() {}
        assert!(!b.try_retry(), "резерв исчерпан");
        for _ in 0..11 {
            b.on_request();
        }
        assert!(b.try_retry(), "одиннадцать запросов дали больше одного повтора");
        assert!(!b.try_retry(), "но не два");
    }
}
