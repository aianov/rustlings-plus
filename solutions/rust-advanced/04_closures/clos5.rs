pub fn retry<F: FnMut() -> Result<u32, String>>(attempts: u32, mut op: F) -> Result<u32, String> {
    let mut last = Err("no attempts".to_string());
    for _ in 0..attempts {
        last = op();
        if last.is_ok() {
            break;
        }
    }
    last
}

pub struct Defer<F: FnOnce()> {
    action: Option<F>,
}

impl<F: FnOnce()> Defer<F> {
    pub fn new(action: F) -> Self {
        Defer { action: Some(action) }
    }

    pub fn run_now(&mut self) {
        if let Some(action) = self.action.take() {
            action();
        }
    }
}

impl<F: FnOnce()> Drop for Defer<F> {
    fn drop(&mut self) {
        self.run_now();
    }
}

fn main() {
    let _d = Defer::new(|| println!("bye"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn retry_stops_at_first_success() {
        let mut calls = 0;
        let result = retry(5, || {
            calls += 1;
            if calls < 3 { Err(format!("fail {calls}")) } else { Ok(7) }
        });
        assert_eq!(result, Ok(7));
        assert_eq!(calls, 3, "после успеха повторять не нужно");
    }

    #[test]
    fn retry_returns_last_error() {
        let mut calls = 0;
        let result = retry(3, || {
            calls += 1;
            Err(format!("fail {calls}"))
        });
        assert_eq!(result, Err("fail 3".to_string()), "возвращается ошибка последней попытки");
        assert_eq!(calls, 3);
        assert_eq!(retry(0, || Ok(1)), Err("no attempts".to_string()), "ноль попыток это ошибка");
    }

    #[test]
    fn defer_runs_on_drop() {
        let ran = Cell::new(0);
        {
            let _d = Defer::new(|| ran.set(ran.get() + 1));
            assert_eq!(ran.get(), 0, "пока Defer жив, действие не выполняется");
        }
        assert_eq!(ran.get(), 1, "при уничтожении действие выполняется");
    }

    #[test]
    fn defer_runs_exactly_once() {
        let ran = Cell::new(0);
        {
            let mut d = Defer::new(|| ran.set(ran.get() + 1));
            d.run_now();
            assert_eq!(ran.get(), 1);
            d.run_now();
            assert_eq!(ran.get(), 1, "повторный run_now ничего не делает");
        }
        assert_eq!(ran.get(), 1, "и при уничтожении действие уже не повторяется");
    }

    #[test]
    fn defer_can_own_data() {
        let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let log2 = log.clone();
        let msg = String::from("closed");
        drop(Defer::new(move || log2.borrow_mut().push(msg)));
        assert_eq!(*log.borrow(), vec!["closed".to_string()], "FnOnce может отдавать захваченные данные");
    }
}
