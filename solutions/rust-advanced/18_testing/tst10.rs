use std::time::{SystemTime, UNIX_EPOCH};

pub trait Clock {
    fn now(&self) -> u64;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
    }
}

pub struct FixedClock(pub u64);

impl Clock for FixedClock {
    fn now(&self) -> u64 {
        self.0
    }
}

pub fn day_key(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

pub struct Report<C: Clock> {
    clock: C,
}

impl<C: Clock> Report<C> {
    pub fn new(clock: C) -> Self {
        Report { clock }
    }

    pub fn file_name(&self) -> String {
        format!("report-{}.txt", day_key(self.clock.now()))
    }

    pub fn is_today(&self, day: &str) -> bool {
        day == day_key(self.clock.now())
    }
}

fn main() {
    println!("{}", Report::new(SystemClock).file_name());
}

#[cfg(test)]
mod tests {
    use super::*;
        fn code_only() -> String {
        SRC.split("#[cfg(test)]")
            .next()
            .unwrap_or("")
            .lines()
            .map(|line| line.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n")
    }
    const SRC: &str = include_str!("tst10.rs");

    const LAST_SECOND_OF_MARCH: u64 = 1_775_001_599;
    const FIRST_SECOND_OF_APRIL: u64 = 1_775_001_601;

    #[test]
    fn day_keys_are_right() {
        assert_eq!(day_key(0), "1970-01-01");
        assert_eq!(day_key(1_709_208_000), "2024-02-29", "високосный день");
        assert_eq!(day_key(1_798_761_599), "2026-12-31");
        assert_eq!(day_key(LAST_SECOND_OF_MARCH), "2026-03-31");
        assert_eq!(day_key(FIRST_SECOND_OF_APRIL), "2026-04-01");
    }

    #[test]
    fn file_name_follows_the_clock() {
        let late = Report::new(FixedClock(LAST_SECOND_OF_MARCH));
        let early = Report::new(FixedClock(FIRST_SECOND_OF_APRIL));
        assert_eq!(late.file_name(), "report-2026-03-31.txt", "отчёт в 23:59:59 принадлежит дню, который показывают часы");
        assert_eq!(early.file_name(), "report-2026-04-01.txt", "две секунды спустя день уже другой");
    }

    #[test]
    fn is_today_follows_the_clock() {
        let report = Report::new(FixedClock(LAST_SECOND_OF_MARCH));
        assert!(report.is_today("2026-03-31"), "сегодня по часам 31 марта");
        assert!(!report.is_today("2026-04-01"));
    }

    #[test]
    fn system_time_is_read_in_one_place_only() {
        let code = code_only();
        assert_eq!(code.matches("SystemTime::now").count(), 1, "SystemTime::now должен остаться только в SystemClock");
    }
}
