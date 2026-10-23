pub mod reviews {
    #[derive(Debug, PartialEq, Eq)]
    pub enum RatingError {
        OutOfRange(u8),
        NotANumber,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Rating(u8);

    impl Rating {
        pub fn new(value: u8) -> Result<Rating, RatingError> {
            if (1..=5).contains(&value) {
                Ok(Rating(value))
            } else {
                Err(RatingError::OutOfRange(value))
            }
        }

        pub fn get(self) -> u8 {
            self.0
        }

        pub fn saturating_add(self, delta: u8) -> Rating {
            Rating(self.0.saturating_add(delta).min(5))
        }

        pub fn saturating_sub(self, delta: u8) -> Rating {
            Rating(self.0.saturating_sub(delta).max(1))
        }
    }

    impl std::str::FromStr for Rating {
        type Err = RatingError;

        fn from_str(s: &str) -> Result<Rating, RatingError> {
            let value: u8 = s.trim().parse().map_err(|_| RatingError::NotANumber)?;
            Rating::new(value)
        }
    }

    pub fn average(ratings: &[Rating]) -> Option<f64> {
        if ratings.is_empty() {
            return None;
        }
        let sum: u32 = ratings.iter().map(|r| u32::from(r.0)).sum();
        Some(f64::from(sum) / ratings.len() as f64)
    }
}

fn main() {
    let r = reviews::Rating::new(4).map(|r| r.get());
    println!("{r:?}");
}

#[cfg(test)]
mod tests {
    use super::reviews::*;

const SRC: &str = include_str!("wsp3.rs");

fn rustc(args: &[&str]) -> Result<(), String> {
    let out = std::process::Command::new("rustc")
        .args(args)
        .output()
        .expect("не удалось запустить rustc: он должен быть в PATH");
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

fn scratch_dir() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rustlings_probe_{}_{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).expect("не удалось создать временную папку");
    dir
}

fn inside(extra: &str) -> Result<(), String> {
    let dir = scratch_dir();
    let file = dir.join("probe.rs");
    std::fs::write(&file, format!("{SRC}\n{extra}\n")).expect("не удалось записать файл");
    let result = rustc(&[
        "--edition", "2024", "--crate-type", "lib", "--crate-name", "probe", "--emit=metadata",
        "-A", "warnings", "-D", "private_interfaces", "-D", "private_bounds",
        "--out-dir", dir.to_str().expect("путь не UTF-8"), file.to_str().expect("путь не UTF-8"),
    ]);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn outside(client: &str) -> Result<(), String> {
    let dir = scratch_dir();
    let lib = dir.join("exercise.rs");
    std::fs::write(&lib, SRC).expect("не удалось записать файл");
    let dir_s = dir.to_str().expect("путь не UTF-8");
    rustc(&[
        "--edition", "2024", "--crate-type", "lib", "--crate-name", "exercise", "-A", "warnings",
        "--out-dir", dir_s, lib.to_str().expect("путь не UTF-8"),
    ])
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

    #[test]
    fn valid_values_are_accepted() {
        for v in 1..=5 {
            assert_eq!(Rating::new(v).map(Rating::get), Ok(v), "рейтинг {v} должен приниматься");
        }
    }

    #[test]
    fn invalid_values_are_rejected() {
        assert_eq!(Rating::new(0).map(Rating::get), Err(RatingError::OutOfRange(0)), "0 не рейтинг");
        assert_eq!(Rating::new(6).map(Rating::get), Err(RatingError::OutOfRange(6)), "6 не рейтинг");
        assert_eq!(Rating::new(255).map(Rating::get), Err(RatingError::OutOfRange(255)), "255 не рейтинг");
    }

    #[test]
    fn methods_keep_the_invariant() {
        let three = Rating::new(3).expect("3 допустимо");
        assert_eq!(three.saturating_add(1).get(), 4, "3 + 1");
        assert_eq!(three.saturating_add(200).get(), 5, "прибавка не должна выводить за пятёрку");
        assert_eq!(three.saturating_sub(1).get(), 2, "3 - 1");
        assert_eq!(three.saturating_sub(200).get(), 1, "вычитание не должно опускать ниже единицы");
    }

    #[test]
    fn parsing_checks_both_syntax_and_range() {
        assert_eq!("4".parse::<Rating>().map(Rating::get), Ok(4));
        assert_eq!(" 2 ".parse::<Rating>().map(Rating::get), Ok(2), "пробелы по краям терпим");
        assert_eq!("9".parse::<Rating>().map(Rating::get), Err(RatingError::OutOfRange(9)));
        assert_eq!("five".parse::<Rating>().map(Rating::get), Err(RatingError::NotANumber));
        assert_eq!("-1".parse::<Rating>().map(Rating::get), Err(RatingError::NotANumber), "минус перед числом для u8 это не число");
    }

    #[test]
    fn average_of_ratings() {
        let rs: Vec<Rating> = [5, 4, 3].iter().map(|v| Rating::new(*v).expect("допустимо")).collect();
        assert_eq!(average(&rs), Some(4.0));
        assert_eq!(average(&[]), None, "у пустого списка среднего нет");
    }

    #[test]
    fn nobody_can_build_a_rating_by_hand() {
        assert!(
            inside("mod other { pub fn f() -> crate::reviews::Rating { crate::reviews::Rating(9) } }").is_err(),
            "соседний модуль собрал Rating(9) напрямую: поле должно быть приватным"
        );
        assert!(
            outside("pub fn f() -> exercise::reviews::Rating { exercise::reviews::Rating(9) }").is_err(),
            "внешний крейт собрал Rating(9) напрямую: поле должно быть приватным"
        );
    }

    #[test]
    fn there_is_no_default_rating() {
        assert!(
            inside("fn f() { let _: crate::reviews::Rating = Default::default(); }").is_err(),
            "у Rating есть Default и он выдаёт Rating(0): уберите derive(Default), значение «по умолчанию» нарушает инвариант"
        );
    }

    #[test]
    fn fields_are_not_reachable_even_for_reading() {
        assert!(
            outside("pub fn f(r: exercise::reviews::Rating) -> u8 { r.0 }").is_err(),
            "значение читается только через get()"
        );
    }

    #[test]
    fn an_honest_value_can_still_be_used_outside() {
        assert!(
            outside(
                "pub fn f() -> Option<u8> { exercise::reviews::Rating::new(3).ok().map(|r| r.get()) }"
            )
            .is_ok(),
            "через Rating::new и get внешний крейт работать обязан"
        );
    }
}
