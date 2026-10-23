use std::collections::HashMap;

pub type Handler = Box<dyn Fn(&str) -> String>;

pub struct Router {
    routes: HashMap<String, Handler>,
}

impl Default for Router {
    fn default() -> Self {
        Router::new()
    }
}

impl Router {
    pub fn new() -> Router {
        Router { routes: HashMap::new() }
    }

    pub fn add(&mut self, name: &str, handler: impl Fn(&str) -> String + 'static) -> bool {
        self.routes.insert(name.to_string(), Box::new(handler)).is_some()
    }

    pub fn call(&self, name: &str, arg: &str) -> Option<String> {
        self.routes.get(name).map(|handler| handler(arg))
    }

    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.routes.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }
}

fn main() {
    let mut r = Router::new();
    r.add("echo", |s| s.to_string());
    println!("{:?}", r.call("echo", "hi"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shout(s: &str) -> String {
        s.to_uppercase()
    }

    #[test]
    fn closures_and_functions_live_together() {
        let mut r = Router::new();
        let suffix = String::from("!");
        assert!(!r.add("shout", shout));
        assert!(!r.add("bang", move |s| format!("{s}{suffix}")));
        assert_eq!(r.call("shout", "hi"), Some("HI".to_string()));
        assert_eq!(r.call("bang", "hi"), Some("hi!".to_string()));
    }

    #[test]
    fn unknown_name_is_none() {
        assert_eq!(Router::new().call("nope", "x"), None);
    }

    #[test]
    fn replacing_reports_and_takes_effect() {
        let mut r = Router::new();
        assert!(!r.add("a", |_| "old".to_string()));
        assert!(r.add("a", |_| "new".to_string()), "второй add с тем же именем заменяет обработчик");
        assert_eq!(r.call("a", ""), Some("new".to_string()));
    }

    #[test]
    fn names_are_sorted() {
        let mut r = Router::new();
        for n in ["zeta", "alpha", "mid"] {
            r.add(n, |s| s.to_string());
        }
        assert_eq!(r.names(), vec!["alpha", "mid", "zeta"]);
    }

    #[test]
    fn handlers_can_keep_state_in_shared_cells() {
        let hits = std::rc::Rc::new(std::cell::Cell::new(0));
        let hits2 = hits.clone();
        let mut r = Router::new();
        r.add("count", move |_| {
            hits2.set(hits2.get() + 1);
            hits2.get().to_string()
        });
        assert_eq!(r.call("count", ""), Some("1".to_string()));
        assert_eq!(r.call("count", ""), Some("2".to_string()));
        assert_eq!(hits.get(), 2);
    }
}
