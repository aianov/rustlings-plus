// Время в tokio живёт в модуле tokio::time: sleep усыпляет задачу (не поток!), а Instant из того
// же модуля это часы рантайма. Тесты тут идут на замороженном времени (start_paused), поэтому
// секунды проходят мгновенно, а замер получается точным.
// TODO: Допиши две функции. `timed_sleep` спит столько миллисекунд и возвращает, сколько
// прошло на самом деле. `backoff` возвращает паузу перед повтором: 100 мс на нулевую попытку,
// и вдвое больше на каждую следующую, но не больше двух секунд. Большие номера попыток не должны
// ни паниковать, ни переполняться.
use std::time::Duration;
use tokio::time::Instant;

async fn timed_sleep(ms: u64) -> Duration {
    todo!()
}

fn backoff(attempt: u32) -> Duration {
    todo!()
}

#[tokio::main]
async fn main() {
    println!("{:?}", timed_sleep(50).await);
    println!("{:?}", backoff(3));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn sleep_is_measured_exactly() {
        assert_eq!(timed_sleep(250).await, Duration::from_millis(250), "на замороженных часах замер точный");
    }

    #[tokio::test(start_paused = true)]
    async fn sleep_zero() {
        assert_eq!(timed_sleep(0).await, Duration::ZERO, "нулевой сон не тратит времени");
    }

    #[test]
    fn backoff_doubles() {
        assert_eq!(backoff(0), Duration::from_millis(100), "первая пауза 100 мс");
        assert_eq!(backoff(1), Duration::from_millis(200), "вторая 200 мс");
        assert_eq!(backoff(4), Duration::from_millis(1600), "пятая 1600 мс");
    }

    #[test]
    fn backoff_is_capped() {
        assert_eq!(backoff(5), Duration::from_millis(2000), "3200 мс обрезается до 2000");
        assert_eq!(backoff(40), Duration::from_millis(2000), "большие попытки не переполняются");
        assert_eq!(backoff(1000), Duration::from_millis(2000), "и очень большие тоже");
        assert_eq!(backoff(62), Duration::from_millis(2000), "сдвиг на 62 ещё помещается в u64, но произведение с 100 теряет старшие биты и даёт ноль: умножай насыщающе");
        assert_eq!(backoff(63), Duration::from_millis(2000), "то же на 63: проверь, что проверка переполнения стоит на умножении, а не только на сдвиге");
    }
}
