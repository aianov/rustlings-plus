
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

fn main() {}

#[derive(Clone, Default)]
pub struct Counter {
    value: Arc<AtomicU64>,
}

impl Counter {
    pub fn inc(&self) {
        self.add(1);
    }

    pub fn add(&self, n: u64) {
        self.value.fetch_add(n, Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

#[derive(Clone, Default)]
pub struct Gauge {
    value: Arc<AtomicI64>,
}

impl Gauge {
    pub fn set(&self, v: i64) {
        self.value.store(v, Ordering::Relaxed);
    }

    pub fn add(&self, delta: i64) {
        self.value.fetch_add(delta, Ordering::Relaxed);
    }

    pub fn inc(&self) {
        self.add(1);
    }

    pub fn dec(&self) {
        self.add(-1);
    }

    pub fn get(&self) -> i64 {
        self.value.load(Ordering::Relaxed)
    }
}

enum Metric {
    Counter(Counter),
    Gauge(Gauge),
}

struct Entry {
    help: String,
    metric: Metric,
}

#[derive(Default)]
pub struct Registry {
    entries: Mutex<BTreeMap<String, Entry>>,
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_' || c == ':') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':')
}

fn escape_help(help: &str) -> String {
    help.replace('\\', "\\\\").replace('\n', "\\n")
}

impl Registry {
    pub fn new() -> Registry {
        Registry::default()
    }

    pub fn counter(&self, name: &str, help: &str) -> Counter {
        assert!(valid_name(name), "недопустимое имя метрики {name:?}");
        let mut entries = self.entries.lock().expect("реестр не отравлен");
        match entries.entry(name.to_string()).or_insert_with(|| Entry { help: help.to_string(), metric: Metric::Counter(Counter::default()) }).metric {
            Metric::Counter(ref c) => c.clone(),
            Metric::Gauge(_) => panic!("метрика {name} уже зарегистрирована как gauge"),
        }
    }

    pub fn gauge(&self, name: &str, help: &str) -> Gauge {
        assert!(valid_name(name), "недопустимое имя метрики {name:?}");
        let mut entries = self.entries.lock().expect("реестр не отравлен");
        match entries.entry(name.to_string()).or_insert_with(|| Entry { help: help.to_string(), metric: Metric::Gauge(Gauge::default()) }).metric {
            Metric::Gauge(ref g) => g.clone(),
            Metric::Counter(_) => panic!("метрика {name} уже зарегистрирована как счётчик"),
        }
    }

    pub fn render(&self) -> String {
        let entries = self.entries.lock().expect("реестр не отравлен");
        let mut out = String::new();
        for (name, entry) in entries.iter() {
            let (kind, value) = match &entry.metric {
                Metric::Counter(c) => ("counter", c.get().to_string()),
                Metric::Gauge(g) => ("gauge", g.get().to_string()),
            };
            out.push_str(&format!("# HELP {name} {}\n# TYPE {name} {kind}\n{name} {value}\n", escape_help(&entry.help)));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_and_gauge_render_with_help_and_type() {
        let reg = Registry::new();
        let posts = reg.counter("posts_created_total", "Сколько постов создано");
        let open = reg.gauge("connections_open", "Открытые соединения");
        posts.inc();
        posts.add(4);
        open.add(10);
        open.dec();
        assert_eq!(
            reg.render(),
            "# HELP connections_open Открытые соединения\n# TYPE connections_open gauge\nconnections_open 9\n\
             # HELP posts_created_total Сколько постов создано\n# TYPE posts_created_total counter\nposts_created_total 5\n",
            "метрики по алфавиту, у каждой HELP, TYPE и значение, каждая строка кончается переводом строки"
        );
    }

    #[test]
    fn gauge_goes_down_and_below_zero() {
        let reg = Registry::new();
        let g = reg.gauge("queue_depth", "Глубина очереди");
        g.set(3);
        g.dec();
        g.dec();
        g.dec();
        g.dec();
        assert_eq!(g.get(), -1, "gauge знаковый: отрицательное значение допустимо и должно показываться");
        assert!(reg.render().contains("queue_depth -1\n"), "отрицательное значение выводится как есть: {}", reg.render());
    }

    #[test]
    fn asking_twice_gives_the_same_metric() {
        let reg = Registry::new();
        let first = reg.counter("requests_total", "Запросы");
        let second = reg.counter("requests_total", "Запросы (другой текст)");
        first.inc();
        second.inc();
        assert_eq!(first.get(), 2, "два вызова с одним именем это один счётчик, а не два с одинаковым именем");
        assert_eq!(reg.render().matches("# TYPE requests_total").count(), 1, "метрика в выводе одна");
    }

    #[test]
    fn help_text_is_escaped_so_it_stays_on_one_line() {
        let reg = Registry::new();
        reg.counter("odd_total", "первая строка\nвторая \\ с обратной чертой");
        assert!(reg.render().contains("# HELP odd_total первая строка\\nвторая \\\\ с обратной чертой\n"), "перевод строки и обратная черта в HELP экранируются: {}", reg.render());
    }

    #[test]
    #[should_panic(expected = "gauge")]
    fn the_same_name_cannot_be_a_counter_and_a_gauge() {
        let reg = Registry::new();
        reg.gauge("things", "Штуки");
        reg.counter("things", "Штуки");
    }

    #[test]
    fn invalid_names_are_rejected_at_registration() {
        for bad in ["", "9lives", "has space", "dash-name", "точка.имя", "quote\""] {
            let result = std::panic::catch_unwind(|| Registry::new().counter(bad, "x"));
            assert!(result.is_err(), "имя {bad:?} недопустимо и должно ловиться сразу, а не ломать вывод у Prometheus");
        }
        for good in ["a", "_x", "http:requests_total", "A1_b2"] {
            Registry::new().gauge(good, "x");
        }
    }

    #[test]
    fn many_threads_lose_no_updates() {
        let reg = Arc::new(Registry::new());
        let counter = reg.counter("hits_total", "Обращения");
        let gauge = reg.gauge("balance", "Баланс");
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let (counter, gauge, barrier) = (counter.clone(), gauge.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    for _ in 0..20_000 {
                        counter.inc();
                        gauge.inc();
                        gauge.add(-2);
                        gauge.inc();
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().expect("поток не упал");
        }
        assert_eq!(counter.get(), 160_000, "счётчик потерял обновления: операция должна быть атомарной");
        assert_eq!(gauge.get(), 0);
        assert!(reg.render().contains("hits_total 160000\n"));
    }

    #[test]
    fn render_while_updating_does_not_deadlock() {
        let reg = Arc::new(Registry::new());
        let c = reg.counter("x_total", "x");
        let r2 = reg.clone();
        let t = std::thread::spawn(move || (0..200).map(|_| r2.render().len()).sum::<usize>());
        for _ in 0..10_000 {
            c.inc();
        }
        assert!(t.join().expect("поток не упал") > 0);
    }
}
