// Воркер получил результат работы и решает судьбу задачи. Таймаут почтовика можно повторить через минуту, а адрес,
// которого не существует, не появится и через час. Цель: `decide` по результату и номеру попытки (первая попытка
// это 1) выбирает действие: готово, повторить через паузу или в мёртвую очередь с причиной. Пауза растёт вдвое
// от секунды, но не дольше минуты и не ломается на больших номерах попыток. Всего попыток пять (MAX_ATTEMPTS).

fn main() {}

#[derive(Debug, Clone, PartialEq)]
pub enum JobError {
    Temporary(String),
    Permanent(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Done,
    Retry { after_ms: u64 },
    Dead { reason: String },
}

pub const MAX_ATTEMPTS: u32 = 5;

pub fn backoff_ms(attempt: u32) -> u64 {
    let _ = attempt;
    todo!()
}

pub fn decide(result: Result<(), JobError>, attempt: u32) -> Action {
    let _ = (result, attempt);
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary() -> Result<(), JobError> {
        Err(JobError::Temporary("таймаут".to_string()))
    }

    #[test]
    fn the_pause_doubles_from_a_second_up_to_a_minute() {
        let pauses: Vec<u64> = (1..=8).map(backoff_ms).collect();
        assert_eq!(pauses, vec![1000, 2000, 4000, 8000, 16_000, 32_000, 60_000, 60_000], "1 с, дальше вдвое, потолок минута");
    }

    #[test]
    fn a_huge_attempt_number_does_not_overflow() {
        for attempt in [40, 63, 64, 65, 100, u32::MAX] {
            assert_eq!(backoff_ms(attempt), 60_000, "попытка {attempt}: пауза упирается в потолок, а не в переполнение");
        }
        assert_eq!(backoff_ms(0), 1000, "нулевой номер не бывает, но паники быть не должно: считаем как первую");
    }

    #[test]
    fn success_is_done() {
        assert_eq!(decide(Ok(()), 1), Action::Done, "успех");
        assert_eq!(decide(Ok(()), 5), Action::Done, "успех на последней попытке тоже успех");
    }

    #[test]
    fn a_temporary_error_is_retried_with_a_growing_pause() {
        for attempt in 1..MAX_ATTEMPTS {
            assert_eq!(decide(temporary(), attempt), Action::Retry { after_ms: backoff_ms(attempt) }, "попытка {attempt} из {MAX_ATTEMPTS}: повтор с паузой по номеру");
        }
    }

    #[test]
    fn the_last_attempt_goes_to_the_dead_queue_with_a_reason() {
        match decide(temporary(), MAX_ATTEMPTS) {
            Action::Dead { reason } => {
                assert!(reason.contains("таймаут"), "в причине должна остаться исходная ошибка: {reason}");
                assert!(reason.contains('5'), "и число попыток, чтобы дежурный понял, что это не первая неудача: {reason}");
            }
            other => panic!("на пятой попытке повторять поздно, а получили {other:?}"),
        }
        assert!(matches!(decide(temporary(), 9), Action::Dead { .. }), "и позже тоже мёртвая очередь, а не новый повтор");
    }

    #[test]
    fn a_permanent_error_is_never_retried() {
        for attempt in [1, 2, 5] {
            assert_eq!(
                decide(Err(JobError::Permanent("адрес не существует".to_string())), attempt),
                Action::Dead { reason: "адрес не существует".to_string() },
                "попытка {attempt}: повтор не поможет, причина остаётся как есть"
            );
        }
    }
}
