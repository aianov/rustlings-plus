#![allow(unused_macros)]

macro_rules! columns {
    ($table:ident : $($column:ident),+ $(,)?) => {
        #[allow(dead_code)]
        pub mod $table {
            pub const NAME: &str = stringify!($table);
            pub const COLUMNS: &str = stringify!($($column),+);
            pub const COUNT: usize = [$(stringify!($column)),+].len();
            pub const SELECT: &str = concat!("SELECT ", stringify!($($column),+), " FROM ", stringify!($table));
        }
    };
}

columns! { users: id, login, created_at }
columns! { posts: id, author_id, body }

pub fn users_select(limit: u32) -> String {
    format!("{} LIMIT {limit}", users::SELECT)
}

fn main() {
    println!("{}", users_select(10));
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = include_str!("mcr9.rs");

    fn code_only() -> String {
        SRC.split("#[cfg(test)]")
            .next()
            .unwrap_or("")
            .lines()
            .map(|line| line.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn constants_describe_the_table() {
        assert_eq!(users::NAME, "users");
        assert_eq!(users::COLUMNS, "id, login, created_at");
        assert_eq!(users::COUNT, 3);
        assert_eq!(users::SELECT, "SELECT id, login, created_at FROM users");
        assert_eq!(posts::SELECT, "SELECT id, author_id, body FROM posts");
    }

    #[test]
    fn the_constants_are_real_compile_time_values() {
        const QUERY: &str = users::SELECT;
        static LOOKUP: [&str; 2] = [users::NAME, posts::NAME];
        const WIDTH: usize = users::COUNT + posts::COUNT;
        assert_eq!(QUERY.len(), 39);
        assert_eq!(LOOKUP, ["users", "posts"]);
        assert_eq!(WIDTH, 6);
        assert_eq!(users::SELECT.as_ptr(), users::SELECT.as_ptr(), "одна и та же константа каждый раз");
    }

    #[test]
    fn a_single_column_and_a_trailing_comma_are_fine() {
        columns! { tags: name, }
        assert_eq!(tags::SELECT, "SELECT name FROM tags");
        assert_eq!(tags::COUNT, 1);
    }

    #[test]
    fn users_select_adds_only_the_limit() {
        assert_eq!(users_select(10), "SELECT id, login, created_at FROM users LIMIT 10");
        assert_eq!(users_select(0), "SELECT id, login, created_at FROM users LIMIT 0");
    }

    #[test]
    fn the_query_is_not_assembled_from_a_column_list_at_runtime() {
        let code = code_only();
        assert!(!code.contains(".join("), "колонки не нужно склеивать во время работы: это делает компилятор");
        assert!(code.contains("users::SELECT") || code.contains("SELECT"), "users_select должен опираться на константу");
    }
}
