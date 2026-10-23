#![allow(unused_macros)]

macro_rules! log_line {
    (warn: $($arg:tt)*) => {
        format!("[warn] {}", format!($($arg)*))
    };
    (error: $($arg:tt)*) => {
        format!("[error] {}", format!($($arg)*))
    };
    ($($arg:tt)*) => {
        format!("[info] {}", format!($($arg)*))
    };
}

macro_rules! kind_of {
    ($l:literal) => {
        "literal"
    };
    ($name:ident) => {
        "name"
    };
    ($e:expr) => {
        "expression"
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
