// Счётчик запросов, флаг остановки и датчик «сколько запросов обрабатывается прямо сейчас». Для таких
// вещей замок избыточен: хватит одного атомарного числа, и поток никогда не заблокируется.
// TODO: Допиши типы. Счётчики на `fetch_add`, остановка на `AtomicBool`, датчик с пиком на `fetch_max`.
// Для `Gauge` напиши ещё `Drop` у `Visit`: когда посетитель уходит, текущее число уменьшается само.
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

#[derive(Default)]
pub struct RequestStats {
    requests: AtomicU64,
    errors: AtomicU64,
    last_id: AtomicU64,
}

#[derive(Default)]
pub struct StopFlag {
    stopped: AtomicBool,
}

#[derive(Default)]
pub struct Gauge {
    current: AtomicUsize,
    peak: AtomicUsize,
}

pub struct Visit<'a> {
    gauge: &'a Gauge,
}

impl RequestStats {
    pub fn record(&self, ok: bool) {
        todo!("одно обращение, а при ok == false ещё и одна ошибка")
    }

    pub fn requests(&self) -> u64 {
        todo!("всего обращений")
    }

    pub fn errors(&self) -> u64 {
        todo!("из них ошибок")
    }

    pub fn next_id(&self) -> u64 {
        todo!("уникальный номер: 1, 2, 3... из любого потока")
    }
}

impl StopFlag {
    pub fn stop(&self) {
        todo!("поднять флаг")
    }

    pub fn is_stopped(&self) -> bool {
        todo!("поднят ли флаг")
    }
}

pub fn run_until_stopped(stop: &StopFlag, mut step: impl FnMut()) -> u64 {
    todo!("вызывать step, пока флаг не поднят; вернуть число вызовов")
}

impl Gauge {
    pub fn enter(&self) -> Visit<'_> {
        todo!("посетитель зашёл: текущее число растёт, пик обновляется")
    }

    pub fn current(&self) -> usize {
        todo!("сколько внутри сейчас")
    }

    pub fn peak(&self) -> usize {
        todo!("максимум за всё время")
    }
}

fn main() {
    let stats = RequestStats::default();
    stats.record(true);
    println!("{}", stats.requests());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    fn within<T: Send + 'static>(secs: u64, what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        match rx.recv_timeout(std::time::Duration::from_secs(secs)) {
            Ok(value) => value,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!("{what}: не уложилось в {secs} с, похоже на зависание"),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!("{what}: поток упал с паникой"),
        }
    }

    #[test]
    fn counters_do_not_lose_updates() {
        let stats = Arc::new(RequestStats::default());
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let stats = Arc::clone(&stats);
                thread::spawn(move || {
                    for k in 0..2000 {
                        stats.record((i + k) % 4 != 0);
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(stats.requests(), 16_000);
        assert_eq!(stats.errors(), 4_000, "ошибка это каждый четвёртый запрос");
    }

    #[test]
    fn ids_are_unique_across_threads() {
        let stats = Arc::new(RequestStats::default());
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let stats = Arc::clone(&stats);
                thread::spawn(move || (0..500).map(|_| stats.next_id()).collect::<Vec<_>>())
            })
            .collect();
        let all: Vec<u64> = handles.into_iter().flat_map(|h| h.join().unwrap()).collect();
        let unique: HashSet<_> = all.iter().collect();
        assert_eq!(unique.len(), 4000, "один номер выдан двум потокам");
        assert_eq!((*all.iter().min().unwrap(), *all.iter().max().unwrap()), (1, 4000));
    }

    #[test]
    fn the_worker_stops_when_the_flag_goes_up() {
        let calls = within(5, "остановка", || {
            let stop = Arc::new(StopFlag::default());
            let worker_stop = Arc::clone(&stop);
            let worker = thread::spawn(move || run_until_stopped(&worker_stop, thread::yield_now));
            thread::sleep(Duration::from_millis(50));
            assert!(!stop.is_stopped());
            stop.stop();
            assert!(stop.is_stopped());
            worker.join().unwrap()
        });
        assert!(calls > 0, "рабочий должен успеть сделать хотя бы шаг");
    }

    #[test]
    fn a_stopped_flag_means_no_steps_at_all() {
        let stop = StopFlag::default();
        stop.stop();
        assert_eq!(run_until_stopped(&stop, || panic!("шагов быть не должно")), 0);
    }

    #[test]
    fn the_gauge_tracks_current_and_peak() {
        let gauge = Gauge::default();
        assert_eq!((gauge.current(), gauge.peak()), (0, 0));
        let a = gauge.enter();
        let b = gauge.enter();
        let c = gauge.enter();
        assert_eq!((gauge.current(), gauge.peak()), (3, 3));
        drop(b);
        drop(a);
        assert_eq!((gauge.current(), gauge.peak()), (1, 3), "пик не уменьшается, текущее уменьшается при выходе");
        drop(c);
        let _d = gauge.enter();
        assert_eq!((gauge.current(), gauge.peak()), (1, 3));
    }

    #[test]
    fn the_gauge_sees_real_concurrency() {
        let gauge = Arc::new(Gauge::default());
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let gauge = Arc::clone(&gauge);
                thread::spawn(move || {
                    let _visit = gauge.enter();
                    thread::sleep(Duration::from_millis(80));
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(gauge.current(), 0, "все ушли, внутри никого");
        assert!(gauge.peak() >= 2, "четыре потока спали одновременно, пик должен это показать");
    }
}
