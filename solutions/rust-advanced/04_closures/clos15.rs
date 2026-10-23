type Listener = Box<dyn FnMut(&Counter)>;

pub struct Counter {
    value: u32,
    listeners: Vec<Listener>,
}

impl Default for Counter {
    fn default() -> Self {
        Counter::new()
    }
}

impl Counter {
    pub fn new() -> Counter {
        Counter { value: 0, listeners: Vec::new() }
    }

    pub fn value(&self) -> u32 {
        self.value
    }

    pub fn listen(&mut self, listener: impl FnMut(&Counter) + 'static) {
        self.listeners.push(Box::new(listener));
    }

    pub fn inc(&mut self) {
        self.value += 1;
        let mut listeners = std::mem::take(&mut self.listeners);
        for listener in listeners.iter_mut() {
            listener(self);
        }
        self.listeners = listeners;
    }
}

fn main() {
    let mut c = Counter::new();
    c.listen(|c| println!("now {}", c.value()));
    c.inc();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn listeners_see_the_new_value_in_order() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut c = Counter::new();
        for tag in ["a", "b"] {
            let seen = Rc::clone(&seen);
            c.listen(move |counter| seen.borrow_mut().push(format!("{tag}{}", counter.value())));
        }
        c.inc();
        c.inc();
        assert_eq!(*seen.borrow(), vec!["a1", "b1", "a2", "b2"], "слушатели идут в порядке добавления и видят уже увеличенное значение");
    }

    #[test]
    fn listeners_survive_notification() {
        let calls = Rc::new(RefCell::new(0));
        let mut c = Counter::new();
        let calls2 = Rc::clone(&calls);
        c.listen(move |_| *calls2.borrow_mut() += 1);
        for _ in 0..5 {
            c.inc();
        }
        assert_eq!(*calls.borrow(), 5, "слушатель должен вызываться при каждом inc, а не пропадать после первого");
        assert_eq!(c.value(), 5);
    }

    #[test]
    fn listener_added_between_incs_is_called_later() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut c = Counter::new();
        c.inc();
        let log2 = Rc::clone(&log);
        c.listen(move |counter| log2.borrow_mut().push(counter.value()));
        c.inc();
        assert_eq!(*log.borrow(), vec![2], "на прошлые inc новый слушатель не откликается");
    }

    #[test]
    fn counter_without_listeners_works() {
        let mut c = Counter::new();
        c.inc();
        assert_eq!(c.value(), 1);
    }
}
