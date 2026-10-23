use anyhow::{Result, bail};
use std::path::Path;

use crate::{exercise::Ctx, journey};

pub struct Violation {
    pub rule: String,
    pub path: Option<String>,
    pub message: String,
}

pub enum Report {
    Clean,
    Violations(Vec<Violation>),
}

pub trait ArchChecker: Sync {
    fn check(
        &self,
        project_dir: &Path,
        rules_toml: &str,
        locale: &str,
    ) -> Result<Vec<Violation>, String>;
}

pub struct Library;

impl ArchChecker for Library {
    fn check(
        &self,
        project_dir: &Path,
        rules_toml: &str,
        locale: &str,
    ) -> Result<Vec<Violation>, String> {
        archcheck::check(project_dir, rules_toml, locale).map(|violations| {
            violations
                .into_iter()
                .map(|v| Violation {
                    rule: v.rule,
                    path: v.path,
                    message: v.message,
                })
                .collect()
        })
    }
}

pub static DEFAULT_CHECKER: Library = Library;

pub fn run(
    checker: &dyn ArchChecker,
    project_dir: &Path,
    rules_toml: &str,
    locale: &str,
) -> Result<Report> {
    if rules_toml.trim().is_empty() {
        return Ok(Report::Clean);
    }
    match checker.check(project_dir, rules_toml, locale) {
        Ok(violations) if violations.is_empty() => Ok(Report::Clean),
        Ok(violations) => Ok(Report::Violations(violations)),
        Err(e) => bail!("archcheck: {e}"),
    }
}

pub fn check_milestone(ctx: &Ctx, ind: usize) -> Result<Report> {
    let course = ctx.course;
    let ex = &course.exercises[ind];
    let Some(rules) = course
        .fs
        .resolve(&format!("{}/arch.toml", ex.src), ctx.loc())
    else {
        return Ok(Report::Clean);
    };
    let Some(dir) = journey::dir_of(course, ex) else {
        return Ok(Report::Clean);
    };
    let rules = String::from_utf8_lossy(rules);
    run(&DEFAULT_CHECKER, &ctx.root.join(dir), &rules, ctx.loc())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;

    impl ArchChecker for Fake {
        fn check(&self, _: &Path, rules: &str, locale: &str) -> Result<Vec<Violation>, String> {
            if rules.contains("bad") {
                return Err("bad rules".to_owned());
            }
            Ok(rules
                .lines()
                .filter(|l| l.starts_with("fail"))
                .map(|l| Violation {
                    rule: "exists".to_owned(),
                    path: Some(l.to_owned()),
                    message: locale.to_owned(),
                })
                .collect())
        }
    }

    #[test]
    fn report_kinds() {
        let dir = Path::new(".");
        assert!(matches!(run(&Fake, dir, "", "ru").unwrap(), Report::Clean));
        assert!(matches!(
            run(&Fake, dir, "ok", "ru").unwrap(),
            Report::Clean
        ));
        let Report::Violations(v) = run(&Fake, dir, "fail1\nfail2", "ru").unwrap() else {
            panic!("expected violations");
        };
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].message, "ru");
        assert!(run(&Fake, dir, "bad", "ru").is_err());
    }

    #[test]
    fn library_checker_works() {
        let dir = std::env::temp_dir().join(format!("rustlings-arch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rules = "[[rules]]\nkind = \"exists\"\npaths = [\"Cargo.toml\"]\nmessage.en = \"no manifest\"\nmessage.ru = \"нет манифеста\"\n";
        let Report::Violations(v) = run(&DEFAULT_CHECKER, &dir, rules, "ru").unwrap() else {
            panic!("expected a violation");
        };
        assert_eq!(v[0].rule, "exists");
        assert!(v[0].message.contains("нет манифеста"));
        std::fs::write(dir.join("Cargo.toml"), "").unwrap();
        assert!(matches!(
            run(&DEFAULT_CHECKER, &dir, rules, "ru").unwrap(),
            Report::Clean
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
