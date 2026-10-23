
use std::future::Future;
use std::time::Duration;
use tokio::time::Instant;

fn main() {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline {
    at: Instant,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DeadlineExceeded;

impl Deadline {
    /// Срок через `budget` от сейчас.
    pub fn after(budget: Duration) -> Deadline {
        Deadline { at: Instant::now() + budget }
    }

    /// Сколько осталось (ноль, если срок вышел).
    pub fn remaining(&self) -> Duration {
        self.at.saturating_duration_since(Instant::now())
    }

    pub fn expired(&self) -> bool {
        self.remaining().is_zero()
    }

    /// Срок для нижнего сервиса: `keep` оставляем себе на упаковку ответа. Раньше сейчас срок не бывает, позже родительского тоже.
    pub fn reserve(&self, keep: Duration) -> Deadline {
        let earliest = self.at.checked_sub(keep).unwrap_or_else(Instant::now);
        Deadline { at: earliest.max(Instant::now()).min(self.at) }
    }

    /// Значение заголовка `x-deadline-ms`: целые миллисекунды остатка, округление вниз.
    pub fn to_header(&self) -> String {
        self.remaining().as_millis().to_string()
    }

    /// Разбор заголовка. Мусор (не число, отрицательное, дробное) это `None`: вызывающий возьмёт свой срок по умолчанию.
    /// Чужой запрос не может выдать себе больше времени, чем разрешено нами: остаток не длиннее `cap`.
    pub fn from_header(value: &str, cap: Duration) -> Option<Deadline> {
        let ms: u64 = value.trim().parse().ok()?;
        Some(Deadline::after(Duration::from_millis(ms).min(cap)))
    }
}

/// Выполняет `work` не дольше срока. Если срок вышел до начала, `work` даже не опрашивается.
pub async fn run_with<F: Future>(deadline: Deadline, work: F) -> Result<F::Output, DeadlineExceeded> {
    if deadline.expired() {
        return Err(DeadlineExceeded);
    }
    tokio::time::timeout_at(deadline.at, work).await.map_err(|_| DeadlineExceeded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[tokio::test(start_paused = true)]
    async fn remaining_goes_down_with_the_clock() {
        let d = Deadline::after(MS(1000));
        assert_eq!(d.remaining(), MS(1000), "сразу после создания остаётся весь бюджет");
        tokio::time::advance(MS(300)).await;
        assert_eq!(d.remaining(), MS(700), "через 300 мс должно остаться 700 мс");
        assert!(!d.expired());
        tokio::time::advance(MS(2000)).await;
        assert_eq!(d.remaining(), MS(0), "после срока остаток ноль, а не паника при вычитании");
        assert!(d.expired(), "срок вышел, expired() должен быть true");
    }

    #[tokio::test(start_paused = true)]
    async fn reserve_shortens_but_never_extends() {
        let d = Deadline::after(MS(1000));
        assert_eq!(d.reserve(MS(200)).remaining(), MS(800), "нижнему сервису достаётся бюджет минус то, что оставили себе");
        assert!(d.reserve(MS(5000)).expired(), "если оставить больше, чем есть, нижнему сервису ничего не остаётся");
        assert_eq!(d.reserve(MS(0)).remaining(), d.remaining(), "резерв ноль равен самому сроку");
        tokio::time::advance(MS(400)).await;
        assert_eq!(d.reserve(MS(100)).remaining(), MS(500), "резерв считается от текущего остатка");
    }

    #[tokio::test(start_paused = true)]
    async fn header_carries_the_remaining_time() {
        let d = Deadline::after(MS(1000));
        tokio::time::advance(MS(250)).await;
        assert_eq!(d.to_header(), "750", "в заголовок идёт остаток на момент отправки, а не исходный бюджет");
        let got = Deadline::from_header("750", Duration::from_secs(5)).expect("число миллисекунд разбирается");
        assert_eq!(got.remaining(), MS(750));
        assert_eq!(Deadline::from_header("  300 ", MS(5000)).map(|d| d.remaining()), Some(MS(300)), "пробелы по краям терпим");
    }

    #[tokio::test(start_paused = true)]
    async fn header_cannot_extend_the_budget() {
        let got = Deadline::from_header("600000", Duration::from_secs(5)).unwrap();
        assert_eq!(got.remaining(), Duration::from_secs(5), "чужой запрос просит десять минут, а мы разрешаем не больше потолка");
        assert!(Deadline::from_header("0", MS(5000)).unwrap().expired(), "ноль миллисекунд это уже вышедший срок, а не отсутствие срока");
    }

    #[tokio::test(start_paused = true)]
    async fn garbage_header_is_none() {
        for bad in ["", "abc", "-1", "1.5", "12ms", "1e3"] {
            assert!(Deadline::from_header(bad, MS(5000)).is_none(), "{bad:?} не число миллисекунд: нужен None, чтобы вызывающий взял срок по умолчанию");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn run_with_cuts_work_exactly_at_the_deadline() {
        let start = Instant::now();
        let d = Deadline::after(MS(500));
        let r = run_with(d, async {
            tokio::time::sleep(Duration::from_secs(60)).await;
            1
        })
        .await;
        assert_eq!(r, Err(DeadlineExceeded));
        assert_eq!(start.elapsed(), MS(500), "обрыв должен быть ровно в срок, а не раньше и не позже");
        assert_eq!(run_with(Deadline::after(MS(500)), async { 7 }).await, Ok(7), "успевшая работа возвращает результат");
    }

    #[tokio::test(start_paused = true)]
    async fn expired_deadline_does_not_start_the_work() {
        let started = Arc::new(AtomicBool::new(false));
        let flag = started.clone();
        let d = Deadline::after(MS(100));
        tokio::time::advance(MS(200)).await;
        let r = run_with(d, async move {
            flag.store(true, Ordering::SeqCst);
        })
        .await;
        assert_eq!(r, Err(DeadlineExceeded));
        assert!(!started.load(Ordering::SeqCst), "срок вышел до начала, а работа всё равно началась: проверь срок до первого опроса");
    }
}
