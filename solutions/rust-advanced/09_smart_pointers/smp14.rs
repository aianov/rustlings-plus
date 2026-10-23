use std::cell::{Cell, OnceCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, OnceLock};

static LOADS: AtomicUsize = AtomicUsize::new(0);
static BUILDS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, PartialEq)]
pub struct Config {
    pub name: String,
    pub workers: usize,
}

fn load_config() -> Config {
    LOADS.fetch_add(1, Ordering::SeqCst);
    Config { name: "app".to_string(), workers: 4 }
}

type Ops = HashMap<&'static str, fn(i64, i64) -> i64>;

fn build_ops() -> Ops {
    BUILDS.fetch_add(1, Ordering::SeqCst);
    let mut ops: Ops = HashMap::new();
    ops.insert("add", |a, b| a + b);
    ops.insert("mul", |a, b| a * b);
    ops
}

pub fn loads() -> usize {
    LOADS.load(Ordering::SeqCst)
}

pub fn builds() -> usize {
    BUILDS.load(Ordering::SeqCst)
}

static CONFIG: OnceLock<Config> = OnceLock::new();
static OPS: LazyLock<Ops> = LazyLock::new(build_ops);

pub fn config() -> &'static Config {
    CONFIG.get_or_init(load_config)
}

pub fn apply(op: &str, a: i64, b: i64) -> Option<i64> {
    OPS.get(op).map(|f| f(a, b))
}

pub struct Report {
    numbers: Vec<u32>,
    total: OnceCell<u64>,
    computed: Cell<u32>,
}

impl Report {
    pub fn new(numbers: Vec<u32>) -> Report {
        Report { numbers, total: OnceCell::new(), computed: Cell::new(0) }
    }

    pub fn total(&self) -> u64 {
        *self.total.get_or_init(|| {
            self.computed.set(self.computed.get() + 1);
            self.numbers.iter().map(|n| u64::from(*n)).sum()
        })
    }

    pub fn computed(&self) -> u32 {
        self.computed.get()
    }
}

fn main() {
    println!("{} {:?}", config().name, apply("add", 1, 2));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn config_is_loaded_once_and_shared() {
        let handles: Vec<_> = (0..8).map(|_| thread::spawn(|| config() as *const Config as usize)).collect();
        let addresses: Vec<usize> = handles.into_iter().map(|h| h.join().expect("поток")).collect();
        assert!(addresses.windows(2).all(|w| w[0] == w[1]), "все потоки должны получить одну и ту же конфигурацию");
        assert_eq!(loads(), 1, "даже при гонке потоков читать с диска нужно ровно один раз");
        assert_eq!(config(), &Config { name: "app".to_string(), workers: 4 });
    }

    #[test]
    fn operations_table_is_built_once() {
        assert_eq!(apply("add", 2, 3), Some(5));
        assert_eq!(apply("mul", 4, 5), Some(20));
        assert_eq!(apply("pow", 2, 3), None);
        assert_eq!(builds(), 1, "таблицу строят при первом обращении, и только один раз");
    }

    #[test]
    fn report_total_is_computed_on_demand() {
        let r = Report::new(vec![1, 2, 3, u32::MAX]);
        assert_eq!(r.computed(), 0, "пока никто не просил, ничего не считаем");
        assert_eq!(r.total(), 6 + u64::from(u32::MAX));
        assert_eq!(r.total(), 6 + u64::from(u32::MAX));
        assert_eq!(r.computed(), 1, "второе чтение берёт запомненное значение");
        assert_eq!(Report::new(vec![]).total(), 0);
    }
}
