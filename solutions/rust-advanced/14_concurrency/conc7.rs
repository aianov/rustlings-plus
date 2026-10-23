use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, PartialEq)]
pub enum StockError {
    NotEnough { have: u32, want: u32 },
}

pub struct Inventory {
    stock: Mutex<HashMap<String, u32>>,
}

impl Inventory {
    pub fn new(items: &[(&str, u32)]) -> Self {
        let stock = items.iter().map(|(name, count)| (name.to_string(), *count)).collect();
        Inventory { stock: Mutex::new(stock) }
    }

    pub fn available(&self, item: &str) -> u32 {
        self.stock.lock().unwrap().get(item).copied().unwrap_or(0)
    }

    pub fn total(&self) -> u32 {
        self.stock.lock().unwrap().values().sum()
    }

    pub fn reserve(&self, item: &str, count: u32) -> Result<u32, StockError> {
        let mut stock = self.stock.lock().unwrap();
        let have = stock.get(item).copied().unwrap_or(0);
        if have < count {
            return Err(StockError::NotEnough { have, want: count });
        }
        let left = stock.get_mut(item).expect("товар только что нашли");
        *left -= count;
        Ok(*left)
    }

    pub fn restock_report(&self, item: &str, count: u32) -> String {
        let mut stock = self.stock.lock().unwrap();
        *stock.entry(item.to_string()).or_insert(0) += count;
        let here = stock[item];
        let total: u32 = stock.values().sum();
        format!("{item}: {here} шт., всего на складе {total}")
    }
}

fn main() {
    let inventory = Inventory::new(&[("кружка", 3)]);
    println!("{:?}", inventory.reserve("кружка", 1));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

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
    fn reserving_takes_items_from_the_stock() {
        within(2, "reserve", || {
            let inventory = Inventory::new(&[("кружка", 5), ("блокнот", 2)]);
            assert_eq!(inventory.reserve("кружка", 2), Ok(3));
            assert_eq!(inventory.available("кружка"), 3);
            assert_eq!(inventory.reserve("блокнот", 2), Ok(0));
        });
    }

    #[test]
    fn not_enough_reports_what_is_there() {
        within(2, "reserve при нехватке", || {
            let inventory = Inventory::new(&[("кружка", 1)]);
            assert_eq!(inventory.reserve("кружка", 4), Err(StockError::NotEnough { have: 1, want: 4 }));
            assert_eq!(inventory.reserve("чайник", 1), Err(StockError::NotEnough { have: 0, want: 1 }));
            assert_eq!(inventory.available("кружка"), 1, "неудачное резервирование ничего не списывает");
        });
    }

    #[test]
    fn the_report_counts_the_new_arrival() {
        let report = within(2, "restock_report", || {
            let inventory = Inventory::new(&[("кружка", 5), ("блокнот", 2)]);
            inventory.restock_report("кружка", 3)
        });
        assert_eq!(report, "кружка: 8 шт., всего на складе 10");
    }

    #[test]
    fn concurrent_buyers_never_oversell() {
        let sold = within(3, "покупатели", || {
            let inventory = Arc::new(Inventory::new(&[("кружка", 5)]));
            let handles: Vec<_> = (0..16)
                .map(|_| {
                    let inventory = Arc::clone(&inventory);
                    thread::spawn(move || inventory.reserve("кружка", 1).is_ok())
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).filter(|ok| *ok).count()
        });
        assert_eq!(sold, 5, "продать можно ровно столько, сколько лежит на складе");
    }
}
