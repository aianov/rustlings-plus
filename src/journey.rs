use anyhow::{Context, Result, bail};
use std::{collections::BTreeMap, path::Path};

use crate::{
    course::{Course, Exercise, Kind},
    exercise::Ctx,
    fsutil::{move_path, new_backup_dir, write_file, write_if_changed, write_if_missing},
    i18n::{fill, tr},
    layout,
};

pub type Snapshot = BTreeMap<String, &'static [u8]>;

pub fn dir_of<'a>(course: &'a Course, ex: &Exercise) -> Option<&'a str> {
    ex.journey.map(|j| course.journeys[j].dir.as_str())
}

pub fn position(course: &Course, ind: usize) -> Option<(usize, usize)> {
    course.milestone_position(ind)
}

pub fn is_first(course: &Course, ind: usize) -> bool {
    position(course, ind).is_none_or(|(_, pos)| pos == 0)
}

pub fn previous(course: &Course, ind: usize) -> Option<usize> {
    let (journey, pos) = position(course, ind)?;
    let prev_name = course.journeys[journey]
        .milestones
        .get(pos.checked_sub(1)?)?;
    course.find(prev_name)
}

fn under_dir(ex: &Exercise, sub: &str) -> String {
    format!("{}/{sub}", ex.src)
}

fn own_test(ctx: &Ctx, ind: usize) -> Option<String> {
    let ex = &ctx.course.exercises[ind];
    let file = format!("{}.rs", ex.name);
    ctx.course
        .fs
        .tree(&under_dir(ex, "inject"), ctx.loc())
        .into_iter()
        .map(|(rel, _)| rel)
        .find(|rel| rel.rsplit('/').next() == Some(file.as_str()))
}

pub fn ahead_of(ctx: &Ctx, ind: usize) -> Option<usize> {
    let course = ctx.course;
    let (journey, pos) = position(course, ind)?;
    let dir = ctx.root.join(dir_of(course, &course.exercises[ind])?);
    if !dir.is_dir() {
        return None;
    }
    course.journeys[journey].milestones[pos + 1..]
        .iter()
        .rev()
        .filter_map(|name| course.find(name))
        .filter(|&m| course.exercises[m].kind == Kind::Milestone)
        .find(|&m| own_test(ctx, m).is_some_and(|rel| dir.join(rel).is_file()))
}

pub fn ahead_notice(course: &Course, ind: usize, later: usize, cli: bool) -> String {
    let ex = &course.exercises[ind];
    let dir = format!("{}/", dir_of(course, ex).unwrap_or_default());
    let later = &course.exercises[later].name;
    if cli {
        fill(tr().journey_ahead_cli, &[&dir, later, &ex.name])
    } else {
        fill(tr().journey_ahead_watch, &[&dir, later])
    }
}

pub fn apply_scaffold(ctx: &Ctx, ind: usize) -> Result<usize> {
    let course = ctx.course;
    let ex = &course.exercises[ind];
    let Some(dir) = dir_of(course, ex) else {
        return Ok(0);
    };
    let dir = ctx.root.join(dir);
    std::fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;
    let mut written = 0;
    for (rel, content) in course.fs.tree(&under_dir(ex, "scaffold"), ctx.loc()) {
        if write_if_missing(dir.join(&rel), content)? {
            written += 1;
        }
    }
    layout::write_vscode_settings(course, &ctx.root)?;
    Ok(written)
}

pub fn write_inject(ctx: &Ctx, ind: usize) -> Result<()> {
    let course = ctx.course;
    let ex = &course.exercises[ind];
    let Some(dir) = dir_of(course, ex) else {
        return Ok(());
    };
    let dir = ctx.root.join(dir);
    for (rel, content) in course.fs.tree(&under_dir(ex, "inject"), ctx.loc()) {
        write_if_changed(dir.join(&rel), content)?;
    }
    Ok(())
}

fn apply_deleted(snapshot: &mut Snapshot, deleted: &[u8]) {
    for line in String::from_utf8_lossy(deleted).lines() {
        let path = line.trim().trim_start_matches("./").trim_end_matches('/');
        if path.is_empty() || path.starts_with('#') {
            continue;
        }
        let prefix = format!("{path}/");
        snapshot.retain(|p, _| p != path && !p.starts_with(&prefix));
    }
}

pub fn apply_overlay(snapshot: &mut Snapshot, overlay: Vec<(String, &'static [u8])>) {
    if let Some((_, deleted)) = overlay.iter().find(|(p, _)| p == ".deleted") {
        apply_deleted(snapshot, deleted);
    }
    for (path, content) in overlay {
        if path != ".deleted" {
            snapshot.insert(path, content);
        }
    }
}

pub fn snapshot(ctx: &Ctx, journey: usize, upto: usize) -> Result<Option<Snapshot>> {
    let course = ctx.course;
    let mut snapshot = Snapshot::new();
    for name in course.journeys[journey].milestones.iter().take(upto + 1) {
        let Some(ind) = course.find(name) else {
            return Ok(None);
        };
        let ex = &course.exercises[ind];
        if ex.kind != Kind::Milestone {
            bail!("`{name}` is listed as a milestone but it is not one");
        }
        let overlay = course.fs.tree(&ex.sol, ctx.loc());
        if overlay.is_empty() {
            return Ok(None);
        }
        apply_overlay(&mut snapshot, overlay);
    }
    Ok(Some(snapshot))
}

pub fn snapshot_upto(ctx: &Ctx, ind: usize) -> Result<Option<Snapshot>> {
    let Some((journey, pos)) = position(ctx.course, ind) else {
        let ex = &ctx.course.exercises[ind];
        let overlay = ctx.course.fs.tree(&ex.sol, ctx.loc());
        if overlay.is_empty() {
            return Ok(None);
        }
        let mut snapshot = Snapshot::new();
        apply_overlay(&mut snapshot, overlay);
        return Ok(Some(snapshot));
    };
    snapshot(ctx, journey, pos)
}

pub fn prerequisites_done(course: &Course, done: &[bool], ind: usize) -> bool {
    let Some((journey, pos)) = position(course, ind) else {
        return true;
    };
    course.journeys[journey].milestones[..pos]
        .iter()
        .filter_map(|name| course.find(name))
        .all(|i| done[i])
}

pub fn baseline_offer(course: &Course, ind: usize, cli: bool) -> Option<String> {
    let ex = &course.exercises[ind];
    let prev = previous(course, ind)?;
    let prev_name = &course.exercises[prev].name;
    let dir = format!("{}/", dir_of(course, ex)?);
    Some(if cli {
        fill(tr().baseline_offer_cli, &[&dir, &ex.name, prev_name])
    } else {
        fill(tr().baseline_offer_watch, &[&dir, prev_name])
    })
}

pub fn open(ctx: &Ctx, done: &[bool], ind: usize, cli: bool) -> Result<Option<String>> {
    let course = ctx.course;
    let ex = &course.exercises[ind];
    if ex.kind != Kind::Milestone {
        return Ok(None);
    }
    let Some(dir) = dir_of(course, ex) else {
        return Ok(None);
    };
    if let Some(later) = ahead_of(ctx, ind) {
        return Ok(cli.then(|| ahead_notice(course, ind, later, true)));
    }
    let exists = ctx.root.join(dir).is_dir();
    if is_first(course, ind) || exists {
        apply_scaffold(ctx, ind)?;
    }
    if is_first(course, ind) || (exists && prerequisites_done(course, done, ind)) {
        return Ok(None);
    }
    Ok(baseline_offer(course, ind, cli))
}

pub fn baseline(ctx: &Ctx, ind: usize) -> Result<String> {
    let course = ctx.course;
    let ex = &course.exercises[ind];
    let Some(dir) = dir_of(course, ex) else {
        bail!("{}", fill(tr().not_milestone, &[&ex.name]));
    };
    let base = match (previous(course, ind), position(course, ind)) {
        (Some(prev), Some((journey, pos))) => {
            let Some(snapshot) = snapshot(ctx, journey, pos - 1)? else {
                bail!(
                    "{}",
                    fill(tr().baseline_missing, &[&course.exercises[prev].name])
                );
            };
            Some((prev, snapshot))
        }
        _ => None,
    };

    let project = ctx.root.join(dir);
    let mut backup_note = None;
    if project.exists() {
        let backup = new_backup_dir(&ctx.root).join(dir);
        move_path(&project, &backup)?;
        backup_note = Some(backup);
    }
    std::fs::create_dir_all(&project)
        .with_context(|| format!("Failed to create {}", project.display()))?;

    let dir_display = format!("{dir}/");
    let mut message = match &base {
        Some((prev, snapshot)) => {
            for (path, content) in snapshot {
                write_file(project.join(path), content)?;
            }
            fill(
                tr().baseline_done,
                &[&dir_display, &course.exercises[*prev].name],
            )
        }
        None => fill(tr().baseline_empty, &[&dir_display]),
    };
    apply_scaffold(ctx, ind)?;
    if let Some(backup) = backup_note {
        let shown = backup
            .strip_prefix(&ctx.root)
            .unwrap_or(Path::new(&backup))
            .display()
            .to_string();
        message.push('\n');
        message.push_str(&fill(tr().migrate_backup, &[&shown]));
    }
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{course::Course, embedded::ContentFs, i18n::Locale};

    fn course() -> &'static Course {
        let files: Vec<(&'static str, &'static [u8])> = vec![
            (
                "course/course.toml",
                b"format_version = 2\n[[tracks]]\nid = \"t\"\nchapters = [\"c\"]\n[[journeys]]\nid = \"backend\"\nmilestones = [\"m1\", \"m2\", \"m3\", \"m4\"]\n",
            ),
            (
                "exercises/t/c/chapter.toml",
                b"[chapter]\ntitle = \"c\"\n[[exercises]]\nname = \"m1\"\nkind = \"milestone\"\n[[exercises]]\nname = \"m2\"\nkind = \"milestone\"\n[[exercises]]\nname = \"m3\"\nkind = \"milestone\"\n[[exercises]]\nname = \"m4\"\nkind = \"milestone\"\n",
            ),
            ("exercises/t/c/m1/README.md", b"m1"),
            ("exercises/t/c/m1/scaffold/src/main.rs", b"fn main() {}\n"),
            ("exercises/t/c/m2/README.md", b"m2"),
            ("exercises/t/c/m2/inject/tests/m2.rs", b"// m2 test\n"),
            ("exercises/t/c/m2/inject/tests/m2.ru.rs", b"// m2 tect\n"),
            ("exercises/t/c/m2/scaffold/src/lib.rs", b"// lib\n"),
            ("exercises/t/c/m3/README.md", b"m3"),
            ("exercises/t/c/m4/README.md", b"m4"),
            ("solutions/t/c/m1/Cargo.toml", b"[workspace]\n"),
            ("solutions/t/c/m1/src/main.rs", b"fn main() {}\n"),
            ("solutions/t/c/m1/src/old.rs", b"old\n"),
            ("solutions/t/c/m2/.deleted", b"src/old.rs\n# comment\n"),
            ("solutions/t/c/m2/src/lib.rs", b"pub fn x() {}\n"),
            ("solutions/t/c/m2/tests/m2.rs", b"// m2 test\n"),
        ];
        let fs = Box::leak(Box::new(ContentFs::from_entries(&files)));
        Box::leak(Box::new(Course::load(fs).unwrap()))
    }

    fn temp_root(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("rustlings-journey-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn snapshots_fold_overlays() {
        let course = course();
        let ctx = Ctx::new(course, Locale::En, temp_root("snap"));
        let s1 = snapshot(&ctx, 0, 0).unwrap().unwrap();
        assert_eq!(
            s1.keys().collect::<Vec<_>>(),
            ["Cargo.toml", "src/main.rs", "src/old.rs"]
        );
        let s2 = snapshot(&ctx, 0, 1).unwrap().unwrap();
        assert!(snapshot(&ctx, 0, 3).unwrap().is_none());
        assert_eq!(
            s2.keys().collect::<Vec<_>>(),
            ["Cargo.toml", "src/lib.rs", "src/main.rs", "tests/m2.rs"]
        );
        assert!(snapshot(&ctx, 0, 2).unwrap().is_none());
        assert_eq!(previous(course, 1), Some(0));
        assert!(is_first(course, 0));
        assert!(!is_first(course, 2));
        std::fs::remove_dir_all(&ctx.root).unwrap();
    }

    #[test]
    fn open_scaffold_inject_and_baseline() {
        let course = course();
        let root = temp_root("open");
        let ctx = Ctx::new(course, Locale::Ru, root.clone());

        assert!(open(&ctx, &[false; 4], 0, false).unwrap().is_none());
        assert!(root.join("backend/src/main.rs").is_file());

        std::fs::write(root.join("backend/mine.rs"), "mine").unwrap();
        let offer = open(&ctx, &[false; 4], 1, true).unwrap().unwrap();
        assert!(offer.contains("rustlings journey baseline m2"));
        assert!(root.join("backend/src/lib.rs").is_file());
        assert!(
            open(&ctx, &[true, false, false, false], 1, true)
                .unwrap()
                .is_none()
        );

        write_inject(&ctx, 1).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("backend/tests/m2.rs")).unwrap(),
            "// m2 tect\n"
        );

        let message = baseline(&ctx, 1).unwrap();
        assert!(message.contains("m1"));
        assert!(root.join("backend/src/old.rs").is_file());
        assert!(!root.join("backend/mine.rs").exists());
        let backups = std::fs::read_dir(&root)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".rustlings-backup-")
            })
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        assert!(backups[0].path().join("backend/mine.rs").is_file());

        let message = baseline(&ctx, 0).unwrap();
        assert!(message.contains("backend/"));
        assert!(!root.join("backend/src/old.rs").exists());
        assert!(root.join("backend/src/main.rs").is_file());

        assert!(baseline(&ctx, 2).is_ok());
        assert!(root.join("backend/tests/m2.rs").is_file());
        assert!(baseline(&ctx, 3).is_err());

        assert_eq!(ahead_of(&ctx, 0), Some(1));
        assert_eq!(ahead_of(&ctx, 1), None);
        let notice = open(&ctx, &[false; 4], 0, true).unwrap().unwrap();
        assert!(notice.contains("m2") && notice.contains("rustlings reset m1"));
        assert!(open(&ctx, &[false; 4], 0, false).unwrap().is_none());
        baseline(&ctx, 0).unwrap();
        assert_eq!(ahead_of(&ctx, 0), None);

        std::fs::remove_dir_all(&root).unwrap();
    }
}
