use std::cell::RefCell;
use std::rc::Rc;

pub type Log = Rc<RefCell<Vec<String>>>;

pub struct Span {
    name: &'static str,
    log: Log,
}

impl Span {
    pub fn enter(name: &'static str, log: &Log) -> Span {
        log.borrow_mut().push(format!("enter {name}"));
        Span { name, log: Rc::clone(log) }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        self.log.borrow_mut().push(format!("exit {}", self.name));
    }
}

pub fn note(log: &Log, text: &str) {
    log.borrow_mut().push(text.to_string());
}

pub fn process(log: &Log, items: &[u32]) -> u32 {
    let _span = Span::enter("process", log);
    let mut total = 0;
    for item in items {
        let _item = Span::enter("item", log);
        if *item == 0 {
            note(log, "zero, stop");
            return total;
        }
        total += item;
        note(log, "added");
    }
    total
}

fn main() {
    let log: Log = Rc::default();
    println!("{}", process(&log, &[1, 2]));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> Log {
        Rc::new(RefCell::new(Vec::new()))
    }

    #[test]
    fn spans_nest_around_the_work() {
        let log = fresh();
        assert_eq!(process(&log, &[10, 20]), 30);
        assert_eq!(
            *log.borrow(),
            vec!["enter process", "enter item", "added", "exit item", "enter item", "added", "exit item", "exit process"],
            "`exit` должен идти после работы, которую охраняет Span"
        );
    }

    #[test]
    fn empty_input_has_only_the_outer_span() {
        let log = fresh();
        assert_eq!(process(&log, &[]), 0);
        assert_eq!(*log.borrow(), vec!["enter process", "exit process"]);
    }

    #[test]
    fn outer_span_closes_last() {
        let log = fresh();
        process(&log, &[1]);
        assert_eq!(log.borrow().last().map(String::as_str), Some("exit process"), "внешний охранник уничтожается последним");
    }

    #[test]
    fn early_exit_still_closes_every_span() {
        let log = fresh();
        assert_eq!(process(&log, &[5, 0, 7]), 5);
        assert_eq!(
            *log.borrow(),
            vec!["enter process", "enter item", "added", "exit item", "enter item", "zero, stop", "exit item", "exit process"],
            "при раннем выходе охранники закрываются сами, в порядке от внутреннего к внешнему"
        );
    }
}
