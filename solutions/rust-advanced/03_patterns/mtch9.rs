pub mod store {
    #[derive(Debug, Clone, PartialEq)]
    #[non_exhaustive]
    pub enum StoreError {
        NotFound,
        PermissionDenied,
        Locked { retry_after: u32 },
        Conflict { existing: String },
        Corrupted,
    }

    pub fn http_status(error: &StoreError) -> u16 {
        match error {
            StoreError::NotFound => 404,
            StoreError::PermissionDenied => 403,
            StoreError::Locked { .. } => 423,
            StoreError::Conflict { .. } => 409,
            StoreError::Corrupted => 500,
        }
    }

    pub fn is_retryable(error: &StoreError) -> bool {
        match error {
            StoreError::Locked { .. } => true,
            StoreError::NotFound
            | StoreError::PermissionDenied
            | StoreError::Conflict { .. }
            | StoreError::Corrupted => false,
        }
    }
}

fn main() {
    println!("{}", store::http_status(&store::StoreError::NotFound));
}

#[cfg(test)]
mod tests {
    use super::store::*;

    const SRC: &str = include_str!("mtch9.rs");

    fn all() -> Vec<StoreError> {
        vec![
            StoreError::NotFound,
            StoreError::PermissionDenied,
            StoreError::Locked { retry_after: 5 },
            StoreError::Conflict { existing: "a".into() },
            StoreError::Corrupted,
        ]
    }

    #[test]
    fn statuses() {
        let got: Vec<u16> = all().iter().map(http_status).collect();
        assert_eq!(got, vec![404, 403, 423, 409, 500], "коды в порядке NotFound, PermissionDenied, Locked, Conflict, Corrupted");
    }

    #[test]
    fn retry_policy() {
        let got: Vec<bool> = all().iter().map(is_retryable).collect();
        assert_eq!(got, vec![false, false, true, false, false], "повторять стоит только Locked; Conflict и Corrupted не повторяют");
    }

    #[test]
    fn no_catch_all_arms_remain() {
        let prod = SRC.split("#[cfg(test)]").next().expect("файл не пуст");
        let wildcard = prod
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .any(|l| l.trim_start().starts_with("_ =>") || l.contains(" _ =>"));
        assert!(!wildcard, "в функциях осталась ветка `_ =>`: она проглотит следующий добавленный вариант");
    }

    fn rustc(args: &[&str]) -> Result<(), String> {
        let out = std::process::Command::new("rustc")
            .args(args)
            .output()
            .expect("не удалось запустить rustc: он должен быть в PATH");
        if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).to_string()) }
    }

    fn outside(client: &str) -> Result<(), String> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("rustlings_mtch9_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).expect("не удалось создать временную папку");
        let dir_s = dir.to_str().expect("путь не UTF-8");
        let lib = dir.join("exercise.rs");
        std::fs::write(&lib, SRC).expect("не удалось записать файл");
        rustc(&["--edition", "2024", "--crate-type", "lib", "--crate-name", "exercise", "-A", "warnings", "--out-dir", dir_s, lib.to_str().expect("путь не UTF-8")])
            .map_err(|e| format!("сам файл не собирается как библиотека:\n{e}"))?;
        let bin = dir.join("client.rs");
        std::fs::write(&bin, client).expect("не удалось записать файл");
        let rlib = dir.join("libexercise.rlib");
        let result = rustc(&[
            "--edition", "2024", "--crate-type", "lib", "--crate-name", "client", "--emit=metadata", "-A", "warnings",
            "--extern", &format!("exercise={}", rlib.to_str().expect("путь не UTF-8")),
            "--out-dir", dir_s, bin.to_str().expect("путь не UTF-8"),
        ]);
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    const STRICT_CLIENT: &str = "use exercise::store::StoreError as E;
        pub fn f(e: &E) -> u8 { match e { E::NotFound => 1, E::PermissionDenied => 2, E::Locked { .. } => 3, E::Conflict { .. } => 4, E::Corrupted => 5 } }";

    const RELAXED_CLIENT: &str = "use exercise::store::StoreError as E;
        pub fn f(e: &E) -> u8 { match e { E::NotFound => 1, E::Locked { .. } => 3, _ => 0 } }";

    #[test]
    fn foreign_code_must_keep_a_fallback_arm() {
        assert!(
            outside(STRICT_CLIENT).is_err(),
            "чужой крейт исчерпал все варианты без запасной ветки и собрался: тип не помечен как расширяемый, новый вариант сломает чужой код"
        );
    }

    #[test]
    fn foreign_code_with_a_fallback_still_works() {
        assert!(outside(RELAXED_CLIENT).is_ok(), "match с запасной веткой снаружи обязан собираться");
    }
}
