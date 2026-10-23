// Сервер запускает три части, и каждая пишет в журнал при уничтожении (`Noisy`). Завершаться они должны
// строго в таком порядке: сначала слушатель (перестаём принимать запросы), потом рабочие (доделывают начатое),
// потом база (закрывается последней). Сейчас порядок обратный, база закрывается под рабочими и теряет
// записи. Rust уничтожает поля структуры в порядке объявления, а локальные переменные в обратном порядке
// объявления. Ещё одна функция должна снять блокировку до записи «commit», а не после.
// TODO: Поменяй порядок так, чтобы журналы совпали с требуемыми. Сами `Noisy` и тесты не трогай.
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
    db: Noisy,
    workers: Noisy,
    listener: Noisy,
}

impl Server {
    pub fn start(log: &Log) -> Server {
        Server { db: Noisy::new("db", log), workers: Noisy::new("workers", log), listener: Noisy::new("listener", log) }
    }
}

pub fn run_session(log: &Log) {
    let cache = Noisy::new("cache", log);
    let conn = Noisy::new("conn", log);
    let auth = Noisy::new("auth", log);
    note(log, "working");
    let _ = (&cache, &conn, &auth);
}

pub fn finish(log: &Log) {
    let lock = Noisy::new("lock", log);
    note(log, "commit");
    let _ = &lock;
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
