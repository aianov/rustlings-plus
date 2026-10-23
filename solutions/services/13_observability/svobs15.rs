
fn main() {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub good: u64,
    pub total: u64,
}

#[derive(Debug, PartialEq)]
pub enum SloError {
    NoBucketAt(f64),
    Malformed,
}

pub const PAGE_BURN: f64 = 14.4;

pub fn availability(w: &Window) -> f64 {
    if w.total == 0 { 1.0 } else { w.good.min(w.total) as f64 / w.total as f64 }
}

pub fn burn_rate(w: &Window, slo: f64) -> f64 {
    let bad_share = 1.0 - availability(w);
    let budget = 1.0 - slo;
    if bad_share == 0.0 {
        0.0
    } else if budget <= 0.0 {
        f64::INFINITY
    } else {
        bad_share / budget
    }
}

pub fn budget_left(slo: f64, expected_total: u64, bad_so_far: u64) -> f64 {
    let allowed = (1.0 - slo) * expected_total as f64;
    if allowed <= 0.0 {
        return if bad_so_far == 0 { 1.0 } else { f64::NEG_INFINITY };
    }
    1.0 - bad_so_far as f64 / allowed
}

pub fn fast_share(buckets: &[(f64, u64)], threshold: f64) -> Result<f64, SloError> {
    let total = match buckets.last() {
        Some((bound, total)) if bound.is_infinite() => *total,
        _ => return Err(SloError::Malformed),
    };
    let under = buckets.iter().find(|(bound, _)| (bound - threshold).abs() < 1e-9).map(|(_, n)| *n).ok_or(SloError::NoBucketAt(threshold))?;
    if total == 0 { Ok(1.0) } else { Ok(under as f64 / total as f64) }
}

pub fn should_page(short: &Window, long: &Window, slo: f64) -> bool {
    burn_rate(short, slo) >= PAGE_BURN && burn_rate(long, slo) >= PAGE_BURN
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(good: u64, total: u64) -> Window {
        Window { good, total }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn availability_is_the_share_of_good_requests_and_silence_is_fine() {
        assert!(close(availability(&w(999, 1000)), 0.999));
        assert_eq!(availability(&w(0, 0)), 1.0, "нет запросов значит нет ошибок, а не 0/0");
        assert_eq!(availability(&w(10, 10)), 1.0);
        assert_eq!(availability(&w(12, 10)), 1.0, "хороших больше, чем всех, это сбой подсчёта: не выше единицы");
    }

    #[test]
    fn burn_rate_one_means_exactly_on_plan() {
        assert!(close(burn_rate(&w(999, 1000), 0.999), 1.0), "ошибок ровно столько, сколько разрешено: скорость 1");
        assert!(close(burn_rate(&w(990, 1000), 0.999), 10.0), "ошибок в десять раз больше: скорость 10");
        assert_eq!(burn_rate(&w(1000, 1000), 0.999), 0.0);
        assert_eq!(burn_rate(&w(0, 0), 0.999), 0.0, "пустое окно не горит и не даёт NaN");
    }

    #[test]
    fn a_perfect_promise_has_no_budget_at_all() {
        assert_eq!(burn_rate(&w(999, 1000), 1.0), f64::INFINITY, "при SLO 100% любая ошибка сжигает весь бюджет");
        assert_eq!(burn_rate(&w(1000, 1000), 1.0), 0.0);
        assert_eq!(budget_left(1.0, 1000, 0), 1.0);
        assert_eq!(budget_left(1.0, 1000, 1), f64::NEG_INFINITY);
    }

    #[test]
    fn budget_left_is_a_share_that_can_go_negative() {
        assert!(close(budget_left(0.999, 1_000_000, 0), 1.0));
        assert!(close(budget_left(0.999, 1_000_000, 250), 0.75), "разрешено 1000 ошибок, ушло 250");
        assert!(close(budget_left(0.999, 1_000_000, 1000), 0.0));
        assert!(close(budget_left(0.999, 1_000_000, 1500), -0.5), "потрачено полтора бюджета: минус полбюджета");
    }

    #[test]
    fn the_fast_share_reads_a_cumulative_bucket() {
        let buckets = [(0.1, 60), (0.3, 90), (1.0, 98), (f64::INFINITY, 100)];
        assert_eq!(fast_share(&buckets, 0.3), Ok(0.9), "90 из 100 ответов уложились в 0.3 с");
        assert_eq!(fast_share(&buckets, 1.0), Ok(0.98));
        assert_eq!(fast_share(&[(0.3, 0), (f64::INFINITY, 0)], 0.3), Ok(1.0), "без запросов все «быстрые»");
    }

    #[test]
    fn a_threshold_between_bounds_cannot_be_answered_honestly() {
        let buckets = [(0.1, 60), (0.3, 90), (f64::INFINITY, 100)];
        assert_eq!(fast_share(&buckets, 0.2), Err(SloError::NoBucketAt(0.2)), "порог 0.2 лежит внутри бакета: ответа нет, гадать нельзя");
        assert_eq!(fast_share(&[(0.3, 90)], 0.3), Err(SloError::Malformed), "нет бакета +Inf: общее число неизвестно");
        assert_eq!(fast_share(&[], 0.3), Err(SloError::Malformed));
    }

    #[test]
    fn paging_needs_both_windows_to_burn() {
        let slo = 0.999;
        let hot = w(98, 100);
        let calm = w(10_000 - 5, 10_000);
        let burning_long = w(98_500, 100_000);
        assert!(should_page(&hot, &burning_long, slo), "оба окна горят быстрее 14.4: будить");
        assert!(!should_page(&hot, &calm, slo), "короткий всплеск, длинное окно спокойно: не будить");
        assert!(!should_page(&calm, &burning_long, slo), "давно починено: короткое окно уже чистое, не будить");
        assert!(!should_page(&w(0, 0), &w(0, 0), slo), "тишина не повод будить");
    }

    #[test]
    fn the_page_threshold_sits_at_fourteen_point_four() {
        let slo = 0.99;
        let (under, over) = (w(8_570, 10_000), w(8_550, 10_000));
        assert!(!should_page(&under, &under, slo), "скорость 14.3 ещё не повод будить: {}", burn_rate(&under, slo));
        assert!(should_page(&over, &over, slo), "скорость 14.5 уже повод: {}", burn_rate(&over, slo));
    }
}
