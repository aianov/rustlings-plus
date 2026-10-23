pub type Command = fn(&str) -> String;
pub type Table = Vec<(String, Box<dyn Fn(&str) -> String>)>;

fn upper(s: &str) -> String {
    s.to_uppercase()
}

fn lower(s: &str) -> String {
    s.to_lowercase()
}

fn reverse(s: &str) -> String {
    s.chars().rev().collect()
}

pub fn builtins() -> Vec<(&'static str, Command)> {
    vec![("upper", upper), ("lower", lower), ("reverse", reverse), ("echo", |s| s.to_string())]
}

pub fn run_builtin(name: &str, arg: &str) -> Option<String> {
    builtins().into_iter().find(|(n, _)| *n == name).map(|(_, command)| command(arg))
}

pub fn with_prefix(prefix: &str) -> Table {
    let mut table: Table = builtins()
        .into_iter()
        .map(|(name, command)| (name.to_string(), Box::new(command) as Box<dyn Fn(&str) -> String>))
        .collect();
    let prefix = prefix.to_string();
    table.push(("tag".to_string(), Box::new(move |s| format!("{prefix}{s}"))));
    table
}

fn main() {
    println!("{:?}", run_builtin("upper", "hi"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_order_and_names() {
        let names: Vec<&str> = builtins().iter().map(|(n, _)| *n).collect();
        assert_eq!(names, vec!["upper", "lower", "reverse", "echo"]);
    }

    #[test]
    fn builtins_work() {
        assert_eq!(run_builtin("upper", "Rust ок"), Some("RUST ОК".to_string()));
        assert_eq!(run_builtin("lower", "ABC"), Some("abc".to_string()));
        assert_eq!(run_builtin("reverse", "абв"), Some("вба".to_string()), "реверс по символам, а не по байтам");
        assert_eq!(run_builtin("echo", "same"), Some("same".to_string()));
        assert_eq!(run_builtin("tag", "x"), None, "tag нет в таблице встроенных");
        assert_eq!(run_builtin("nope", "x"), None);
    }

    #[test]
    fn table_is_plain_function_pointers() {
        let first: Command = builtins()[0].1;
        assert_eq!(first("a"), "A", "элементы таблицы это обычные fn-указатели");
        assert_eq!(std::mem::size_of::<Command>(), std::mem::size_of::<usize>(), "указатель на функцию занимает одно слово");
    }

    #[test]
    fn extended_table_has_a_capturing_command() {
        let table = with_prefix("> ");
        let names: Vec<&str> = table.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["upper", "lower", "reverse", "echo", "tag"]);
        let tag = &table.iter().find(|(n, _)| n == "tag").expect("команда tag должна быть").1;
        assert_eq!(tag("hello"), "> hello");
        let up = &table[0].1;
        assert_eq!(up("hi"), "HI", "встроенные команды работают и в расширенной таблице");
    }

    #[test]
    fn tables_with_different_prefixes_do_not_interfere() {
        let a = with_prefix("A:");
        let b = with_prefix("B:");
        assert_eq!(a[4].1("x"), "A:x");
        assert_eq!(b[4].1("x"), "B:x");
    }
}
