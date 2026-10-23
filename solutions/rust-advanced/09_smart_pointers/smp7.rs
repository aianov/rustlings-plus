use std::cell::{Cell, RefCell};

pub struct Visitor {
    count: Cell<usize>,
    seen: RefCell<Vec<String>>,
}

impl Visitor {
    pub fn new() -> Visitor {
        Visitor { count: Cell::new(0), seen: RefCell::new(Vec::new()) }
    }

    pub fn visit(&self, name: &str) -> usize {
        self.seen.borrow_mut().push(name.to_string());
        self.count.set(self.count.get() + 1);
        self.count.get()
    }

    pub fn try_visit(&self, name: &str) -> Result<usize, &'static str> {
        let mut seen = self.seen.try_borrow_mut().map_err(|_| "busy")?;
        seen.push(name.to_string());
        self.count.set(self.count.get() + 1);
        Ok(self.count.get())
    }

    pub fn count(&self) -> usize {
        self.count.get()
    }

    pub fn last(&self) -> Option<String> {
        self.seen.borrow().last().cloned()
    }

    pub fn with_seen<R>(&self, f: impl FnOnce(&[String]) -> R) -> R {
        f(&self.seen.borrow())
    }

    pub fn drain(&self) -> Vec<String> {
        self.seen.take()
    }
}

impl Default for Visitor {
    fn default() -> Visitor {
        Visitor::new()
    }
}

fn main() {
    let v = Visitor::new();
    v.visit("ann");
    println!("{}", v.count());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_through_a_shared_reference() {
        let v = Visitor::new();
        let (a, b) = (&v, &v);
        assert_eq!(a.visit("ann"), 1);
        assert_eq!(b.visit("bob"), 2);
        assert_eq!(a.count(), 2);
        assert_eq!(b.last().as_deref(), Some("bob"));
    }

    #[test]
    fn closures_can_share_the_visitor() {
        let v = Visitor::new();
        let greet = |name: &str| v.visit(name);
        let log = |name: &str| v.visit(name);
        greet("x");
        log("y");
        assert_eq!(v.with_seen(|names| names.to_vec()), vec!["x", "y"]);
    }

    #[test]
    fn drain_empties_the_list_but_not_the_counter() {
        let v = Visitor::new();
        v.visit("a");
        v.visit("b");
        assert_eq!(v.drain(), vec!["a", "b"]);
        assert_eq!(v.last(), None);
        assert_eq!(v.count(), 2, "счётчик продолжает считать всех, кто приходил");
        assert_eq!(v.visit("c"), 3);
    }

    #[test]
    fn try_visit_reports_busy_instead_of_panicking() {
        let v = Visitor::new();
        v.visit("a");
        let inner = v.with_seen(|names| {
            assert_eq!(names.len(), 1);
            v.try_visit("late")
        });
        assert_eq!(inner, Err("busy"), "пока список читается, запись должна отказать, а не паниковать");
        assert_eq!(v.count(), 1, "отказавшая запись ничего не меняет");
        assert_eq!(v.try_visit("ok"), Ok(2), "после чтения список свободен");
    }
}
