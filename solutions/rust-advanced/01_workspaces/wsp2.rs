pub mod school {
    pub mod grading {
        pub fn average(grades: &[u8]) -> f64 {
            if grades.is_empty() {
                return 0.0;
            }
            grades.iter().map(|g| f64::from(*g)).sum::<f64>() / grades.len() as f64
        }

        pub(super) fn curve(grade: u8) -> u8 {
            grade.saturating_add(5).min(100)
        }

        pub(crate) fn audit_log(message: &str) -> String {
            format!("[audit] {message}")
        }

        pub fn passed(grade: u8) -> bool {
            grade >= rules::pass_mark()
        }

        pub mod rules {
            pub(in crate::school::grading) fn pass_mark() -> u8 {
                50
            }

            pub fn grade_label(grade: u8) -> &'static str {
                match grade {
                    90..=100 => "excellent",
                    50..=89 => "ok",
                    _ => "fail",
                }
            }
        }
    }

    pub mod report {
        use super::grading;

        pub fn card(name: &str, grades: &[u8]) -> String {
            let curved: Vec<u8> = grades.iter().map(|g| grading::curve(*g)).collect();
            format!("{name}: {:.1}", grading::average(&curved))
        }
    }
}

mod metrics {
    pub fn record(student: &str) -> String {
        crate::school::grading::audit_log(student)
    }
}

fn main() {
    println!("{}", crate::school::report::card("ann", &[70, 80]));
    println!("{}", metrics::record("ann"));
}

#[cfg(test)]
mod tests {
    use super::*;

const SRC: &str = include_str!("wsp2.rs");

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
    fn grades_are_averaged_and_curved() {
        assert_eq!(school::grading::average(&[70, 80]), 75.0, "среднее двух оценок");
        assert_eq!(school::grading::average(&[]), 0.0, "у пустого списка среднее ноль");
        assert_eq!(school::report::card("ann", &[70, 80]), "ann: 80.0", "карточка считает среднее уже по сдвинутым оценкам");
    }

    #[test]
    fn pass_mark_and_labels() {
        assert!(school::grading::passed(50), "50 это уже зачёт");
        assert!(!school::grading::passed(49), "49 это ещё нет");
        assert_eq!(school::grading::rules::grade_label(95), "excellent");
        assert_eq!(school::grading::rules::grade_label(10), "fail");
    }

    #[test]
    fn metrics_may_use_the_crate_wide_item() {
        assert_eq!(metrics::record("ann"), "[audit] ann", "metrics пользуется audit_log");
        assert!(
            inside("mod other { pub fn f() -> String { crate::school::grading::audit_log(\"x\") } }").is_ok(),
            "audit_log должна быть видна всему крейту (pub(crate))"
        );
    }

    #[test]
    fn curve_is_hidden_from_the_rest_of_the_crate() {
        assert!(
            inside("mod other { pub fn f() -> u8 { crate::school::grading::curve(1) } }").is_err(),
            "curve видна за пределами модуля school: сузь видимость до pub(super)"
        );
    }

    #[test]
    fn pass_mark_is_hidden_from_everyone_but_grading() {
        assert!(
            inside("mod other { pub fn f() -> u8 { crate::school::grading::rules::pass_mark() } }").is_err(),
            "pass_mark видна вне модуля grading"
        );
    }

    #[test]
    fn outside_world_sees_only_the_public_api() {
        assert!(
            outside("pub fn f() -> f64 { exercise::school::grading::average(&[1]) }").is_ok(),
            "average должна быть доступна внешним крейтам"
        );
        assert!(
            outside("pub fn f() -> &'static str { exercise::school::grading::rules::grade_label(1) }").is_ok(),
            "grade_label должна быть доступна внешним крейтам"
        );
        assert!(
            outside("pub fn f() -> String { exercise::school::grading::audit_log(\"x\") }").is_err(),
            "audit_log видна внешним крейтам: pub(crate) закрывает её снаружи"
        );
        assert!(
            outside("pub fn f() -> u8 { exercise::school::grading::curve(1) }").is_err(),
            "curve видна внешним крейтам"
        );
    }
}
