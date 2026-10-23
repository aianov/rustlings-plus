#![allow(unused_macros)]

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Role {
    Admin,
    Moderator,
    Member,
}

macro_rules! check {
    ($cond:expr) => {
        if $cond {
            Ok(())
        } else {
            Err(format!("провал: {} ({}:{})", stringify!($cond), file!(), line!()))
        }
    };
}

macro_rules! here {
    () => {
        module_path!()
    };
}

macro_rules! is_staff {
    ($role:expr) => {
        matches!($role, Role::Admin | Role::Moderator)
    };
}

macro_rules! join_path {
    ($first:literal $(, $rest:literal)* $(,)?) => {
        concat!($first $(, "/", $rest)*)
    };
}

macro_rules! pick_one {
    ($x:expr) => {
        $x
    };
    ($($other:tt)*) => {
        compile_error!("pick_one! ждёт ровно один аргумент")
    };
}

const API_USERS: &str = join_path!("api", "v1", "users");

fn main() {
    println!("{API_USERS} {}", here!());
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = include_str!("mcr7.rs");

    fn compiles_with_lints(extra: &str, deny: &[&str]) -> bool {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("rustlings_probe_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).expect("не удалось создать папку");
        let file = dir.join("probe.rs");
        let code = SRC.split("#[cfg(test)]").next().unwrap_or("");
        std::fs::write(&file, format!("{code}\n{extra}\n")).expect("не удалось записать файл");
        let mut cmd = std::process::Command::new("rustc");
        cmd.args(["--edition", "2024", "--crate-type", "lib", "--emit=metadata", "-A", "warnings"]);
        for lint in deny {
            cmd.args(["-D", lint]);
        }
        let ok = cmd.arg("--out-dir").arg(&dir).arg(&file).output().expect("не удалось запустить rustc").status.success();
        let _ = std::fs::remove_dir_all(&dir);
        ok
    }

    #[test]
    fn check_reports_where_it_was_called() {
        let (result, line) = (check!(1 > 2), line!());
        assert_eq!(result, Err(format!("провал: 1 > 2 ({}:{})", file!(), line)));
        assert_eq!(check!(2 > 1), Ok(()));
    }

    #[test]
    fn check_quotes_the_condition_as_written() {
        let items = [1, 2, 3];
        let err = check!(items.len() == 4).unwrap_err();
        assert!(err.starts_with("провал: items.len() == 4 ("), "{err}");
    }

    #[test]
    fn here_is_the_module_path_of_the_call() {
        assert!(here!().ends_with("tests"), "module_path! вызывается там, где написан here!(), то есть в модуле tests: {}", here!());
    }

    #[test]
    fn is_staff_matches_two_roles() {
        assert!(is_staff!(Role::Admin));
        assert!(is_staff!(Role::Moderator));
        assert!(!is_staff!(Role::Member));
        let roles = [Role::Member, Role::Admin];
        assert!(is_staff!(roles[1]));
    }

    #[test]
    fn join_path_is_a_compile_time_constant() {
        assert_eq!(API_USERS, "api/v1/users");
        const ONE: &str = join_path!("only");
        assert_eq!(ONE, "only");
        const TRAILING: &str = join_path!("a", "b",);
        assert_eq!(TRAILING, "a/b");
    }

    #[test]
    fn pick_one_returns_its_argument() {
        assert_eq!(pick_one!(2 + 3), 5);
        assert_eq!(pick_one!("x"), "x");
    }

    #[test]
    fn pick_one_refuses_wrong_arity_at_compile_time() {
        assert!(compiles_with_lints("fn t() -> u8 { pick_one!(1) }", &[]), "с одним аргументом макрос должен собираться");
        assert!(!compiles_with_lints("fn t() -> u8 { pick_one!() }", &[]), "без аргументов должна быть ошибка сборки");
        assert!(!compiles_with_lints("fn t() -> u8 { pick_one!(1, 2) }", &[]), "с двумя аргументами тоже");
    }
}
