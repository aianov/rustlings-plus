// Два макроса с особыми случаями. `log_line!` форматирует строку журнала с уровнем: без префикса это
// `info`, а `warn: ...` и `error: ...` меняют уровень. `kind_of!` называет вид аргумента: `"literal"`
// для литерала, `"name"` для голого идентификатора, `"expression"` для всего остального.
// Оба при любом вызове работают как общее правило, а особые случаи не срабатывают никогда:
// `log_line!(warn: "диск {}%", 91)` не собирается, а `kind_of!(5)` отвечает `"expression"`.
// TODO: Почини порядок правил.
#![allow(unused_macros)]

macro_rules! log_line {
    ($($arg:tt)*) => {
        format!("[info] {}", format!($($arg)*))
    };
    (warn: $($arg:tt)*) => {
        format!("[warn] {}", format!($($arg)*))
    };
    (error: $($arg:tt)*) => {
        format!("[error] {}", format!($($arg)*))
    };
}

macro_rules! kind_of {
    ($e:expr) => {
        "expression"
    };
    ($name:ident) => {
        "name"
    };
    ($l:literal) => {
        "literal"
    };
}

fn main() {
    println!("{}", log_line!("готово"));
}

#[cfg(test)]
mod tests {
    #[test]
    fn plain_lines_are_info() {
        assert_eq!(log_line!("запущено"), "[info] запущено");
        assert_eq!(log_line!("{} постов", 3), "[info] 3 постов");
    }

    #[test]
    fn warn_and_error_have_their_own_prefixes() {
        assert_eq!(log_line!(warn: "диск {}%", 91), "[warn] диск 91%");
        assert_eq!(log_line!(error: "упало"), "[error] упало");
        let name = "ann";
        assert_eq!(log_line!(error: "{name} вышла"), "[error] ann вышла");
    }

    #[test]
    fn kind_of_checks_the_narrowest_kind_first() {
        let user = 1;
        assert_eq!(kind_of!(5), "literal");
        assert_eq!(kind_of!("text"), "literal");
        assert_eq!(kind_of!(user), "name");
        assert_eq!(kind_of!(user + 1), "expression");
        assert_eq!(kind_of!(vec![1]), "expression");
        let _ = user;
    }
}
