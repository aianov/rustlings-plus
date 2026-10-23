use anyhow::Result;
use crossterm::style::{Attribute, Color, ResetColor, SetAttribute, SetForegroundColor};
use std::{collections::BTreeMap, fs, io, path::Path};

use crate::{
    course::Kind,
    exercise::Ctx,
    i18n::{fill, tr},
    journey, lang, layout,
    term::write_ansi,
};

const CONTEXT: usize = 3;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Line<'a> {
    Same(&'a str),
    Removed(&'a str),
    Added(&'a str),
}

pub fn diff<'a>(yours: &'a str, reference: &'a str) -> Vec<Line<'a>> {
    let a = yours.split('\n').collect::<Vec<_>>();
    let b = reference.split('\n').collect::<Vec<_>>();
    let mut out = Vec::with_capacity(a.len().max(b.len()));
    let (mut i, mut j) = (0, 0);
    for (mi, mj) in lang::matching_lines(&a, &b) {
        while i < mi {
            out.push(Line::Removed(a[i]));
            i += 1;
        }
        while j < mj {
            out.push(Line::Added(b[j]));
            j += 1;
        }
        out.push(Line::Same(b[mj]));
        i += 1;
        j += 1;
    }
    out.extend(a[i..].iter().map(|l| Line::Removed(l)));
    out.extend(b[j..].iter().map(|l| Line::Added(l)));
    out
}

pub fn render_lines(out: &mut Vec<u8>, lines: &[Line]) {
    let changed = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| !matches!(l, Line::Same(_)))
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    if changed.is_empty() {
        return;
    }
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for i in changed {
        let start = i.saturating_sub(CONTEXT);
        let end = (i + CONTEXT + 1).min(lines.len());
        match ranges.last_mut() {
            Some(last) if start <= last.1 => last.1 = end,
            _ => ranges.push((start, end)),
        }
    }
    for (n, (start, end)) in ranges.into_iter().enumerate() {
        if n > 0 {
            write_ansi(out, SetForegroundColor(Color::DarkGrey));
            out.extend_from_slice(b"  ...\n");
            write_ansi(out, ResetColor);
        }
        for line in &lines[start..end] {
            let (prefix, color, text) = match line {
                Line::Same(t) => ("  ", None, t),
                Line::Removed(t) => ("- ", Some(Color::Red), t),
                Line::Added(t) => ("+ ", Some(Color::Green), t),
            };
            if let Some(color) = color {
                write_ansi(out, SetForegroundColor(color));
            }
            out.extend_from_slice(prefix.as_bytes());
            out.extend_from_slice(text.trim_end_matches('\r').as_bytes());
            if color.is_some() {
                write_ansi(out, ResetColor);
            }
            out.push(b'\n');
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Status {
    Changed,
    OnlyReference,
    OnlyYours,
}

#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: String,
    pub status: Status,
    pub yours: Vec<u8>,
    pub reference: Vec<u8>,
}

fn learner_files(dir: &Path) -> io::Result<BTreeMap<String, Vec<u8>>> {
    fn walk(dir: &Path, rel: &str, out: &mut BTreeMap<String, Vec<u8>>) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let rel = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if entry.file_type()?.is_dir() {
                if name != "target" && name != ".git" {
                    walk(&entry.path(), &rel, out)?;
                }
            } else if name != "Cargo.lock" && name != ".DS_Store" {
                out.insert(rel, fs::read(entry.path())?);
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    if dir.is_dir() {
        walk(dir, "", &mut out)?;
    }
    Ok(out)
}

pub fn compare(
    yours: BTreeMap<String, Vec<u8>>,
    reference: BTreeMap<String, Vec<u8>>,
) -> Vec<FileDiff> {
    let mut out = Vec::new();
    for (path, content) in &reference {
        match yours.get(path) {
            Some(mine) if mine == content => (),
            Some(mine) => out.push(FileDiff {
                path: path.clone(),
                status: Status::Changed,
                yours: mine.clone(),
                reference: content.clone(),
            }),
            None => out.push(FileDiff {
                path: path.clone(),
                status: Status::OnlyReference,
                yours: Vec::new(),
                reference: content.clone(),
            }),
        }
    }
    for (path, content) in yours {
        if !reference.contains_key(&path) {
            out.push(FileDiff {
                path,
                status: Status::OnlyYours,
                yours: content,
                reference: Vec::new(),
            });
        }
    }
    out
}

pub fn exercise_diff(ctx: &Ctx, ind: usize) -> Result<Option<Vec<FileDiff>>> {
    let course = ctx.course;
    let ex = &course.exercises[ind];
    let (yours, reference) = match ex.kind {
        Kind::Question => return Ok(None),
        Kind::File => {
            let Some(reference) = course.fs.resolve(&ex.sol, ctx.loc()) else {
                return Ok(None);
            };
            let yours = fs::read(ctx.root.join(&ex.src)).unwrap_or_default();
            let path = ex.src.clone();
            (
                BTreeMap::from([(path.clone(), yours)]),
                BTreeMap::from([(path, reference.to_vec())]),
            )
        }
        Kind::Project => {
            let prefix = format!("{}/", ex.sol);
            let reference = layout::project_files(course, &ex.sol, ctx.locale)
                .into_iter()
                .map(|(p, c)| (p[prefix.len()..].to_owned(), c.to_vec()))
                .collect::<BTreeMap<_, _>>();
            if reference.is_empty() {
                return Ok(None);
            }
            (learner_files(&ctx.root.join(&ex.src))?, reference)
        }
        Kind::Milestone => {
            let Some(snapshot) = journey::snapshot_upto(ctx, ind)? else {
                return Ok(None);
            };
            let Some(dir) = journey::dir_of(course, ex) else {
                return Ok(None);
            };
            let reference = snapshot
                .into_iter()
                .map(|(p, c)| (p, c.to_vec()))
                .collect::<BTreeMap<_, _>>();
            (learner_files(&ctx.root.join(dir))?, reference)
        }
    };
    Ok(Some(compare(yours, reference)))
}

fn status_label(status: Status) -> &'static str {
    match status {
        Status::Changed => tr().diff_changed,
        Status::OnlyReference => tr().diff_only_reference,
        Status::OnlyYours => tr().diff_only_yours,
    }
}

pub fn render_view(out: &mut Vec<u8>, files: Option<&[FileDiff]>, selected: usize, why: &str) {
    let t = tr();
    match files {
        None => {
            out.extend_from_slice(t.diff_no_reference.as_bytes());
            out.push(b'\n');
        }
        Some([]) => {
            write_ansi(out, SetForegroundColor(Color::Green));
            out.extend_from_slice(t.diff_same.as_bytes());
            write_ansi(out, ResetColor);
            out.push(b'\n');
        }
        Some(files) => {
            if files.len() > 1 {
                write_ansi(out, SetAttribute(Attribute::Bold));
                out.extend_from_slice(t.diff_files.as_bytes());
                write_ansi(out, ResetColor);
                out.push(b'\n');
                for (i, file) in files.iter().enumerate() {
                    let marker = if i == selected { ">" } else { " " };
                    let number = if i < 9 {
                        format!("{}", i + 1)
                    } else {
                        " ".to_owned()
                    };
                    out.extend_from_slice(
                        format!(
                            "{marker} {number}) {} ({})\n",
                            file.path,
                            status_label(file.status)
                        )
                        .as_bytes(),
                    );
                }
                out.push(b'\n');
            }
            let file = &files[selected.min(files.len() - 1)];
            write_ansi(out, SetAttribute(Attribute::Bold));
            out.extend_from_slice(fill(t.diff_title, &[&file.path]).as_bytes());
            write_ansi(out, ResetColor);
            out.push(b'\n');
            match (
                String::from_utf8(file.yours.clone()),
                String::from_utf8(file.reference.clone()),
            ) {
                (Ok(yours), Ok(reference)) => render_lines(out, &diff(&yours, &reference)),
                _ => {
                    out.extend_from_slice(status_label(file.status).as_bytes());
                    out.push(b'\n');
                }
            }
        }
    }
    let why = why.trim();
    if !why.is_empty() {
        out.push(b'\n');
        write_ansi(out, SetAttribute(Attribute::Bold));
        write_ansi(out, SetForegroundColor(Color::Cyan));
        out.extend_from_slice(t.why.as_bytes());
        write_ansi(out, ResetColor);
        out.push(b'\n');
        out.extend_from_slice(why.as_bytes());
        out.push(b'\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(lines: &[Line]) -> String {
        let mut out = Vec::new();
        render_lines(&mut out, lines);
        let s = String::from_utf8(out).unwrap();
        let mut plain = String::new();
        let mut in_escape = false;
        for c in s.chars() {
            match (in_escape, c) {
                (false, '\x1b') => in_escape = true,
                (true, 'm') => in_escape = false,
                (true, _) => (),
                (false, c) => plain.push(c),
            }
        }
        plain
    }

    #[test]
    fn line_diff() {
        let d = diff("a\nb\nc", "a\nx\nc");
        assert_eq!(
            d,
            [
                Line::Same("a"),
                Line::Removed("b"),
                Line::Added("x"),
                Line::Same("c")
            ]
        );
        assert!(
            diff("same\n", "same\n")
                .iter()
                .all(|l| matches!(l, Line::Same(_)))
        );
        assert_eq!(diff("", "new"), [Line::Removed(""), Line::Added("new")]);
    }

    #[test]
    fn hunks_with_context() {
        let yours = (0..20)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let reference = yours.replace("l2\n", "L2\n").replace("l17\n", "L17\n");
        let rendered = plain(&diff(&yours, &reference));
        assert!(rendered.contains("- l2\n+ L2\n"));
        assert!(rendered.contains("  ...\n"));
        assert!(rendered.contains("  l14\n"));
        assert!(!rendered.contains("  l9\n"));
        assert!(plain(&diff("a", "a")).is_empty());
    }

    #[test]
    fn compare_trees() {
        let yours = BTreeMap::from([
            ("a.rs".to_owned(), b"1".to_vec()),
            ("b.rs".to_owned(), b"2".to_vec()),
            ("mine.rs".to_owned(), b"3".to_vec()),
        ]);
        let reference = BTreeMap::from([
            ("a.rs".to_owned(), b"1".to_vec()),
            ("b.rs".to_owned(), b"two".to_vec()),
            ("c.rs".to_owned(), b"4".to_vec()),
        ]);
        let files = compare(yours, reference);
        let summary = files
            .iter()
            .map(|f| (f.path.as_str(), f.status))
            .collect::<Vec<_>>();
        assert_eq!(
            summary,
            [
                ("b.rs", Status::Changed),
                ("c.rs", Status::OnlyReference),
                ("mine.rs", Status::OnlyYours)
            ]
        );
    }
}
