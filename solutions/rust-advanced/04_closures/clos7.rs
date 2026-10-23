use std::rc::Rc;

pub fn labelers(prefix: String, ids: &[u32]) -> Vec<Box<dyn Fn() -> String>> {
    let prefix: Rc<str> = Rc::from(prefix);
    let mut out: Vec<Box<dyn Fn() -> String>> = Vec::new();
    for &id in ids {
        let prefix = Rc::clone(&prefix);
        out.push(Box::new(move || format!("{prefix}-{id}")));
    }
    out
}

fn main() {
    for f in labelers("user".to_string(), &[1, 2]) {
        println!("{}", f());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_built_per_id() {
        let fs = labelers("user".to_string(), &[1, 22, 333]);
        let got: Vec<String> = fs.iter().map(|f| f()).collect();
        assert_eq!(got, vec!["user-1", "user-22", "user-333"]);
    }

    #[test]
    fn handlers_are_independent_and_repeatable() {
        let fs = labelers("p".to_string(), &[5, 6]);
        assert_eq!(fs[1](), "p-6");
        assert_eq!(fs[0](), "p-5");
        assert_eq!(fs[1](), "p-6", "повторный вызов даёт тот же результат");
    }

    #[test]
    fn no_ids_no_handlers() {
        assert!(labelers("x".to_string(), &[]).is_empty());
    }
}
