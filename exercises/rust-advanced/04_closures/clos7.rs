// Фабрика обработчиков: для каждого идентификатора делает замыкание, которое выдаёт метку вида
// `префикс-id`. Префикс общий, пришёл строкой. Компилятор жалуется, что значение перемещено в замыкание
// «в предыдущей итерации цикла», и показывает на `prefix`. Обработчики должны пережить функцию.
// TODO: Почини `labelers`. Каждое замыкание должно давать верную метку, независимо от остальных.

pub fn labelers(prefix: String, ids: &[u32]) -> Vec<Box<dyn Fn() -> String>> {
    let mut out: Vec<Box<dyn Fn() -> String>> = Vec::new();
    for &id in ids {
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
