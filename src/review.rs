use anyhow::{Context, Result, bail};
use crossterm::{
    QueueableCommand,
    style::{Color, ResetColor, SetForegroundColor},
};
use std::{
    fmt::Write as _,
    io::{StdoutLock, Write},
    path::Path,
};

use crate::{
    app_state::{AppState, Extras, Review},
    course::{Course, Kind},
    exercise::{self, FileTarget, OUTPUT_CAPACITY, Outcome},
    fsutil::{day_number, remove_dir_if_exists, today, today_number, write_file, write_if_changed},
    i18n::{fill, tr},
    layout::{BASICS_LINTS, GENERATED_HEADER, render_key, render_string, render_value},
};

pub const MIN_AGE_DAYS: i64 = 3;
pub const REMIND_EVERY_DAYS: i64 = 3;
pub const MIN_SET: usize = 3;
pub const MAX_SET: usize = 5;
pub const PACKAGE: &str = "review-exercises";
const MANIFEST: &str = "tracks/review/Cargo.toml";

pub fn bin_name(name: &str) -> String {
    format!("review-{name}")
}

pub fn file_path(date: &str, name: &str) -> String {
    format!("review/{date}/{name}.rs")
}

pub fn candidates(
    course: &Course,
    done: impl Fn(usize) -> bool,
    extras: &Extras,
    today: i64,
) -> Vec<usize> {
    let mut list = course
        .exercises
        .iter()
        .enumerate()
        .filter(|(ind, ex)| ex.kind == Kind::File && done(*ind))
        .filter_map(|(ind, ex)| {
            let solved = extras.solved.get(&ex.name).and_then(|d| day_number(d))?;
            (today - solved >= MIN_AGE_DAYS).then_some((solved, ind))
        })
        .collect::<Vec<_>>();
    list.sort_unstable();
    list.into_iter().take(MAX_SET).map(|(_, ind)| ind).collect()
}

fn app_candidates(app: &AppState) -> Vec<usize> {
    candidates(
        app.course(),
        |i| app.is_done(i),
        app.extras(),
        today_number(),
    )
}

pub fn reminder(app: &AppState) -> Option<&'static str> {
    if app.extras().review.is_some() {
        return Some(tr().review_active);
    }
    let recent = app
        .extras()
        .last_review
        .as_deref()
        .and_then(day_number)
        .is_some_and(|d| today_number() - d < REMIND_EVERY_DAYS);
    if recent || app_candidates(app).len() < MIN_SET {
        return None;
    }
    Some(tr().review_reminder)
}

fn merge_spec(into: &mut toml::Value, other: &toml::Value) {
    let as_table = |v: &toml::Value| match v {
        toml::Value::String(version) => {
            let mut t = toml::Table::new();
            t.insert("version".to_owned(), toml::Value::String(version.clone()));
            t
        }
        toml::Value::Table(t) => t.clone(),
        _ => toml::Table::new(),
    };
    let mut merged = as_table(into);
    let other = as_table(other);
    let mut features = merged
        .get("features")
        .and_then(|f| f.as_array())
        .cloned()
        .unwrap_or_default();
    if let Some(more) = other.get("features").and_then(|f| f.as_array()) {
        for f in more {
            if !features.contains(f) {
                features.push(f.clone());
            }
        }
    }
    if !features.is_empty() {
        merged.insert("features".to_owned(), toml::Value::Array(features));
    }
    for (k, v) in other {
        merged.entry(k).or_insert(v);
    }
    *into = toml::Value::Table(merged);
}

pub fn manifest(course: &Course, names: &[(String, String)]) -> String {
    let mut deps = toml::Table::new();
    let mut all_basics = true;
    let mut seen_tracks = Vec::new();
    for (name, _) in names {
        let Some(ind) = course.find(name) else {
            continue;
        };
        let track = &course.tracks[course.exercises[ind].track];
        all_basics &= track.id == "rust-basics";
        if seen_tracks.contains(&track.id) {
            continue;
        }
        seen_tracks.push(track.id.clone());
        for (dep, spec) in &track.dependencies {
            match deps.get_mut(dep) {
                Some(existing) => merge_spec(existing, spec),
                None => {
                    deps.insert(dep.clone(), spec.clone());
                }
            }
        }
    }
    let mut out = format!(
        "{GENERATED_HEADER}\n[package]\nname = \"{PACKAGE}\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\nautobins = false\nautoexamples = false\nautotests = false\nautobenches = false\n"
    );
    for (name, path) in names {
        let _ = write!(
            out,
            "\n[[bin]]\nname = {}\npath = {}\n",
            render_string(&bin_name(name)),
            render_string(&format!("../../{path}")),
        );
    }
    out.push_str("\n[dependencies]\n");
    for (dep, spec) in &deps {
        let _ = writeln!(
            out,
            "{} = {}",
            render_key(dep),
            render_value(spec, "../../support")
        );
    }
    if all_basics && !names.is_empty() {
        out.push('\n');
        out.push_str(BASICS_LINTS);
    }
    out
}

pub fn sync_manifest(course: &Course, root: &Path, extras: &Extras) -> Result<()> {
    let names = extras
        .review
        .as_ref()
        .map(|review| {
            review
                .names
                .iter()
                .map(|n| (n.clone(), file_path(&review.date, n)))
                .filter(|(_, path)| root.join(path).is_file())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if names.is_empty() {
        return remove_dir_if_exists(root.join("tracks/review"));
    }
    write_if_changed(root.join(MANIFEST), manifest(course, &names).as_bytes())?;
    Ok(())
}

fn target(app: &AppState, review: &Review, name: &str) -> Option<FileTarget> {
    let ctx = app.ctx();
    let ind = ctx.course.find(name)?;
    let ex = &ctx.course.exercises[ind];
    let mut target = FileTarget::of(ctx, ex);
    let dir = ctx.root.join(format!("review/{}", review.date));
    target.inputs = ex
        .input_files
        .iter()
        .filter_map(|input| {
            let src = format!(
                "{}/{input}",
                ctx.course.chapters[ex.chapter].exercises_dir()
            );
            ctx.course
                .fs
                .resolve(&src, ctx.loc())
                .map(|content| (dir.join(input), content))
        })
        .collect();
    target.package = PACKAGE.to_owned();
    target.bin = bin_name(name);
    target.cwd = dir;
    target.tidy = "tracks/review/../../".to_owned();
    Some(target)
}

pub fn create(app: &mut AppState) -> Result<Option<Review>> {
    let picked = app_candidates(app);
    if picked.len() < MIN_SET {
        return Ok(None);
    }
    let date = today();
    let ctx = app.ctx();
    let mut names = Vec::new();
    for ind in picked {
        let ex = app.exercise(ind);
        let content = ctx
            .course
            .fs
            .resolve(&ex.src, ctx.loc())
            .with_context(|| format!("{} is missing in the course", ex.src))?;
        write_file(ctx.root.join(file_path(&date, &ex.name)), content)?;
        names.push(ex.name.clone());
    }
    let review = Review { date, names };
    let stored = review.clone();
    app.update_extras(|e| e.review = Some(stored))?;
    sync_manifest(app.course(), &app.ctx().root, app.extras())?;
    Ok(Some(review))
}

pub fn print_list(stdout: &mut StdoutLock, review: &Review) -> Result<()> {
    for name in &review.names {
        writeln!(stdout, "  {name}: {}", file_path(&review.date, name))?;
    }
    Ok(())
}

pub fn check(app: &mut AppState, stdout: &mut StdoutLock) -> Result<bool> {
    let Some(review) = app.extras().review.clone() else {
        bail!("{}", tr().review_none);
    };
    sync_manifest(app.course(), &app.ctx().root, app.extras())?;
    let mut solved = 0;
    let mut first_failure: Option<Vec<u8>> = None;
    let mut lines = Vec::new();
    for name in &review.names {
        let Some(target) = target(app, &review, name) else {
            continue;
        };
        let mut output = Vec::with_capacity(OUTPUT_CAPACITY);
        let outcome = exercise::run_file_target(app.ctx(), &target, Some(&mut output))?;
        let ok = outcome == Outcome::Done;
        if ok {
            solved += 1;
        } else if first_failure.is_none() {
            first_failure = Some(output);
        }
        lines.push((ok, name.clone()));
    }
    if let Some(output) = first_failure {
        stdout.write_all(&output)?;
        stdout.write_all(b"\n")?;
    }
    for (ok, name) in &lines {
        stdout.queue(SetForegroundColor(if *ok {
            Color::Green
        } else {
            Color::Red
        }))?;
        write!(stdout, "  {} {name}", if *ok { "[x]" } else { "[ ]" })?;
        stdout.queue(ResetColor)?;
        writeln!(stdout, "  {}", file_path(&review.date, name))?;
    }
    let total = review.names.len();
    writeln!(
        stdout,
        "\n{}",
        fill(tr().review_status, &[&review.date, &solved, &total])
    )?;
    if solved == total {
        app.update_extras(|e| {
            e.review = None;
            e.last_review = Some(today());
        })?;
        sync_manifest(app.course(), &app.ctx().root, app.extras())?;
        stdout.queue(SetForegroundColor(Color::Green))?;
        writeln!(stdout, "{}", tr().review_done)?;
        stdout.queue(ResetColor)?;
        return Ok(true);
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::course::tests::fixture_course;

    #[test]
    fn picks_old_solved_file_exercises() {
        let course = fixture_course();
        let mut extras = Extras::default();
        extras
            .solved
            .insert("intro1".to_owned(), "2026-10-01".to_owned());
        extras
            .solved
            .insert("intro2".to_owned(), "2026-10-19".to_owned());
        extras
            .solved
            .insert("arch1".to_owned(), "2026-09-01".to_owned());
        extras
            .solved
            .insert("proj".to_owned(), "2026-09-01".to_owned());
        let today = day_number("2026-10-20").unwrap();
        let picked = candidates(course, |_| true, &extras, today);
        assert_eq!(picked, [4, 0]);
        assert!(candidates(course, |i| i != 4, &extras, today) == [0]);
    }

    #[test]
    fn merged_manifest() {
        let course = fixture_course();
        let names = vec![
            ("intro1".to_owned(), "review/d/intro1.rs".to_owned()),
            ("arch1".to_owned(), "review/d/arch1.rs".to_owned()),
        ];
        let manifest = manifest(course, &names);
        let parsed = toml::de::from_str::<toml::Table>(&manifest).unwrap();
        assert_eq!(parsed["package"]["name"].as_str(), Some(PACKAGE));
        let bins = parsed["bin"].as_array().unwrap();
        assert_eq!(bins[0]["name"].as_str(), Some("review-intro1"));
        assert_eq!(bins[1]["path"].as_str(), Some("../../review/d/arch1.rs"));
        assert!(parsed["dependencies"].get("tokio").is_some());
        assert!(parsed.get("lints").is_none());

        let mut a = toml::Value::String("1".to_owned());
        let b: toml::Value =
            toml::Value::Table(toml::de::from_str("version = \"1\"\nfeatures = [\"fs\"]").unwrap());
        merge_spec(&mut a, &b);
        assert_eq!(a["features"].as_array().unwrap().len(), 1);
        assert_eq!(a["version"].as_str(), Some("1"));
    }
}
