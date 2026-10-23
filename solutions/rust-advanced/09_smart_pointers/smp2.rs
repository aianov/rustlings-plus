use std::cell::RefCell;
use std::rc::Rc;

pub type Log = Rc<RefCell<Vec<String>>>;

pub struct Noisy {
    name: &'static str,
    log: Log,
}

impl Noisy {
    pub fn new(name: &'static str, log: &Log) -> Noisy {
        Noisy { name, log: Rc::clone(log) }
    }
}

impl Drop for Noisy {
    fn drop(&mut self) {
        self.log.borrow_mut().push(format!("drop {}", self.name));
    }
}

pub fn note(log: &Log, text: &str) {
    log.borrow_mut().push(text.to_string());
}

#[allow(dead_code)]
pub struct Server {
    listener: Noisy,
    workers: Noisy,
    db: Noisy,
}

impl Server {
    pub fn start(log: &Log) -> Server {
        Server { db: Noisy::new("db", log), workers: Noisy::new("workers", log), listener: Noisy::new("listener", log) }
    }
}

pub fn run_session(log: &Log) {
    let auth = Noisy::new("auth", log);
    let conn = Noisy::new("conn", log);
    let cache = Noisy::new("cache", log);
    note(log, "working");
    let _ = (&cache, &conn, &auth);
}

pub fn finish(log: &Log) {
    let lock = Noisy::new("lock", log);
    drop(lock);
    note(log, "commit");
}

fn main() {
    let log: Log = Rc::default();
    drop(Server::start(&log));
    println!("{:?}", log.borrow());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> Log {
        Rc::new(RefCell::new(Vec::new()))
    }

    #[test]
    fn server_shuts_down_from_the_edge_to_the_database() {
        let log = fresh();
        drop(Server::start(&log));
        assert_eq!(*log.borrow(), vec!["drop listener", "drop workers", "drop db"], "поля уничтожаются в порядке объявления в структуре");
    }

    #[test]
    fn session_resources_close_in_reverse_of_opening() {
        let log = fresh();
        run_session(&log);
        assert_eq!(*log.borrow(), vec!["working", "drop cache", "drop conn", "drop auth"], "локальные переменные уничтожаются в обратном порядке объявления: кэш закрывается первым, авторизация последней");
    }

    #[test]
    fn lock_is_released_before_commit() {
        let log = fresh();
        finish(&log);
        assert_eq!(*log.borrow(), vec!["drop lock", "commit"], "блокировку нужно снять до записи commit, а не в конце функции");
    }

    #[test]
    fn nothing_is_dropped_twice() {
        let log = fresh();
        drop(Server::start(&log));
        run_session(&log);
        finish(&log);
        let drops = log.borrow().iter().filter(|e| e.starts_with("drop")).count();
        assert_eq!(drops, 3 + 3 + 1);
    }
}
