use std::cell::RefCell;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Recorder {
    phases: RefCell<Vec<(String, Duration)>>,
}

impl Recorder {
    pub fn record(&self, name: &str, elapsed: Duration) {
        self.phases.borrow_mut().push((name.to_string(), elapsed));
    }

    pub fn phases(&self) -> Vec<(String, Duration)> {
        self.phases.borrow().clone()
    }

    pub fn slowest(&self) -> Option<(String, Duration)> {
        let mut totals: Vec<(String, Duration)> = Vec::new();
        for (name, elapsed) in self.phases.borrow().iter() {
            match totals.iter_mut().find(|(known, _)| known == name) {
                Some((_, total)) => *total += *elapsed,
                None => totals.push((name.clone(), *elapsed)),
            }
        }
        totals.into_iter().max_by_key(|(_, total)| *total)
    }
}

pub struct PhaseTimer<'a> {
    recorder: &'a Recorder,
    name: &'static str,
    started: Instant,
}

impl<'a> PhaseTimer<'a> {
    pub fn start(recorder: &'a Recorder, name: &'static str) -> PhaseTimer<'a> {
        PhaseTimer { recorder, name, started: Instant::now() }
    }
}

impl Drop for PhaseTimer<'_> {
    fn drop(&mut self) {
        self.recorder.record(self.name, self.started.elapsed());
    }
}

pub fn parse_report(recorder: &Recorder, text: &str) -> Result<u32, String> {
    let parts: Vec<&str> = {
        let _timer = PhaseTimer::start(recorder, "split");
        std::thread::sleep(Duration::from_millis(3));
        text.split_whitespace().collect()
    };
    let numbers = {
        let _timer = PhaseTimer::start(recorder, "parse");
        std::thread::sleep(Duration::from_millis(40));
        let mut numbers = Vec::new();
        for part in &parts {
            numbers.push(part.parse::<u32>().map_err(|_| format!("не число: {part}"))?);
        }
        numbers
    };
    let _timer = PhaseTimer::start(recorder, "sum");
    std::thread::sleep(Duration::from_millis(3));
    Ok(numbers.iter().sum())
}

fn main() {
    let recorder = Recorder::default();
    let total = parse_report(&recorder, "1 2 3");
    println!("{total:?}, самая долгая фаза: {:?}", recorder.slowest());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(recorder: &Recorder) -> Vec<String> {
        recorder.phases().into_iter().map(|(name, _)| name).collect()
    }

    #[test]
    fn phases_are_recorded_in_order() {
        let recorder = Recorder::default();
        assert_eq!(parse_report(&recorder, "1 2 3"), Ok(6));
        assert_eq!(names(&recorder), ["split", "parse", "sum"], "каждая фаза записывается, когда заканчивается");
    }

    #[test]
    fn slowest_phase_is_found() {
        let recorder = Recorder::default();
        parse_report(&recorder, "10 20").unwrap();
        let (name, elapsed) = recorder.slowest().expect("записи есть");
        assert_eq!(name, "parse", "в разборе спит 40 мс, в остальных по 3");
        assert!(elapsed >= Duration::from_millis(40), "время фазы не меньше сна внутри неё: {elapsed:?}");
    }

    #[test]
    fn early_return_is_still_recorded() {
        let recorder = Recorder::default();
        assert_eq!(parse_report(&recorder, "1 x 3"), Err("не число: x".to_string()));
        assert_eq!(names(&recorder), ["split", "parse"], "ошибка в фазе parse: её время всё равно записано, а до sum дело не дошло");
    }

    #[test]
    fn drop_records_even_on_panic() {
        let recorder = Recorder::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _timer = PhaseTimer::start(&recorder, "risky");
            panic!("упали посреди фазы");
        }));
        assert!(result.is_err());
        assert_eq!(names(&recorder), ["risky"], "при размотке стека Drop обязан записать фазу");
    }

    #[test]
    fn nested_timers_finish_inside_out() {
        let recorder = Recorder::default();
        {
            let _outer = PhaseTimer::start(&recorder, "outer");
            let _inner = PhaseTimer::start(&recorder, "inner");
        }
        assert_eq!(names(&recorder), ["inner", "outer"], "локальные переменные уничтожаются в обратном порядке создания");
    }

    #[test]
    fn same_name_is_summed() {
        let recorder = Recorder::default();
        recorder.record("io", Duration::from_millis(30));
        recorder.record("cpu", Duration::from_millis(50));
        recorder.record("io", Duration::from_millis(30));
        let (name, total) = recorder.slowest().expect("записи есть");
        assert_eq!((name.as_str(), total), ("io", Duration::from_millis(60)), "две короткие записи io вместе дольше одной длинной cpu");
        assert_eq!(Recorder::default().slowest(), None);
    }
}
