use anyhow::Result;
use std::{collections::HashMap, fs, path::Path};

use crate::{
    course::Kind,
    exercise::{self, Ctx},
    fsutil::write_file,
    i18n::Locale,
    journey, layout,
    originals::Originals,
};

const ALIGNED_EXT: [&str; 5] = [".rs", ".toml", ".proto", ".sql", ".cql"];
const MAX_DP_CELLS: usize = 25_000_000;

#[derive(Default, Debug, PartialEq, Eq)]
pub struct Report {
    pub recreated: usize,
    pub merged: usize,
}

fn strip_cr(line: &str) -> &str {
    line.strip_suffix('\r').unwrap_or(line)
}

fn lcs_middle(a: &[&str], b: &[&str]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 || n.saturating_mul(m) > MAX_DP_CELLS {
        return Vec::new();
    }
    let width = m + 1;
    let mut table = vec![0u32; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i * width + j] = if strip_cr(a[i]) == strip_cr(b[j]) {
                table[(i + 1) * width + j + 1] + 1
            } else {
                table[(i + 1) * width + j].max(table[i * width + j + 1])
            };
        }
    }
    let mut pairs = Vec::with_capacity(table[0] as usize);
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if strip_cr(a[i]) == strip_cr(b[j]) {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if table[(i + 1) * width + j] >= table[i * width + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    pairs
}

pub fn matching_lines(a: &[&str], b: &[&str]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let mut prefix = 0;
    while prefix < n && prefix < m && strip_cr(a[prefix]) == strip_cr(b[prefix]) {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < n - prefix
        && suffix < m - prefix
        && strip_cr(a[n - 1 - suffix]) == strip_cr(b[m - 1 - suffix])
    {
        suffix += 1;
    }
    let mut pairs = (0..prefix).map(|i| (i, i)).collect::<Vec<_>>();
    pairs.extend(
        lcs_middle(&a[prefix..n - suffix], &b[prefix..m - suffix])
            .into_iter()
            .map(|(i, j)| (i + prefix, j + prefix)),
    );
    pairs.extend((0..suffix).map(|k| (n - suffix + k, m - suffix + k)));
    pairs
}

pub fn remap(old_original: &str, new_original: &str, learner: &str) -> Option<String> {
    let a = old_original.split('\n').collect::<Vec<_>>();
    let b = new_original.split('\n').collect::<Vec<_>>();
    if a.len() != b.len() {
        return None;
    }
    let u = learner.split('\n').collect::<Vec<_>>();
    let mut out = u.iter().map(|l| (*l).to_owned()).collect::<Vec<_>>();
    for (i, j) in matching_lines(&a, &u) {
        let translated = strip_cr(b[i]);
        out[j] = if u[j].ends_with('\r') {
            format!("{translated}\r")
        } else {
            translated.to_owned()
        };
    }
    Some(out.join("\n"))
}

fn aligned(path: &str) -> bool {
    ALIGNED_EXT.iter().any(|ext| path.ends_with(ext))
}

enum Action {
    Skip,
    Recreate,
    Merge(String),
}

fn decide(path: &str, old: Option<&[u8]>, new: &[u8], current: &[u8]) -> Action {
    if current == new {
        return Action::Skip;
    }
    if path.ends_with(".md") {
        return Action::Recreate;
    }
    let Some(old) = old else {
        return Action::Skip;
    };
    if current == old {
        return Action::Recreate;
    }
    if !aligned(path) {
        return Action::Skip;
    }
    let (Ok(old), Ok(new), Ok(current)) = (
        str::from_utf8(old),
        str::from_utf8(new),
        str::from_utf8(current),
    ) else {
        return Action::Skip;
    };
    match remap(old, new, current) {
        Some(merged) if merged != current => Action::Merge(merged),
        _ => Action::Skip,
    }
}

pub fn apply(
    report: &mut Report,
    target: &Path,
    rel: &str,
    old: Option<&[u8]>,
    new: &[u8],
) -> Result<()> {
    let Ok(current) = fs::read(target) else {
        return Ok(());
    };
    match decide(rel, old, new, &current) {
        Action::Skip => (),
        Action::Recreate => {
            write_file(target, new)?;
            report.recreated += 1;
        }
        Action::Merge(merged) => {
            write_file(target, merged.as_bytes())?;
            report.merged += 1;
        }
    }
    Ok(())
}

pub fn switch(ctx: &Ctx, done: &[bool], from: Locale, to: Locale) -> Result<Report> {
    let mut report = Report::default();
    if from == to {
        return Ok(report);
    }
    let course = ctx.course;
    let mut originals = Originals::load(&ctx.root);

    for chapter in 0..course.chapters.len() {
        let old = layout::chapter_files(course, chapter, from)
            .into_iter()
            .collect::<HashMap<_, _>>();
        for (path, new) in layout::chapter_files(course, chapter, to) {
            apply(
                &mut report,
                &ctx.root.join(&path),
                &path,
                old.get(&path).copied(),
                new,
            )?;
            if ctx.root.join(&path).exists() {
                originals.set(&path, new);
            }
        }
    }
    originals.save(&ctx.root)?;

    for (ind, ex) in course.exercises.iter().enumerate() {
        if ex.kind != Kind::Milestone {
            continue;
        }
        let Some(dir) = journey::dir_of(course, ex) else {
            continue;
        };
        let project = ctx.root.join(dir);
        if !project.is_dir() {
            continue;
        }
        for sub in ["scaffold", "inject"] {
            let src = format!("{}/{sub}", ex.src);
            let old = course
                .fs
                .tree(&src, from.as_str())
                .into_iter()
                .collect::<HashMap<_, _>>();
            for (rel, new) in course.fs.tree(&src, to.as_str()) {
                apply(
                    &mut report,
                    &project.join(&rel),
                    &rel,
                    old.get(&rel).copied(),
                    new,
                )?;
            }
        }
        let _ = ind;
    }

    let new_ctx = Ctx::new(course, to, ctx.root.clone());
    for (ind, ex) in course.exercises.iter().enumerate() {
        if done[ind] && ctx.root.join(&ex.sol).exists() {
            exercise::write_solution(&new_ctx, ind)?;
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remap_keeps_learner_edits() {
        let a = "// Fix the code.\nfn main() {\n    // TODO: print\n    let x = 1;\n}\n";
        let b = "// Почини код.\nfn main() {\n    // TODO: выведи\n    let x = 1;\n}\n";
        let u = "// Fix the code.\nfn main() {\n    // my note\n    let x = 2;\n    println!(\"{x}\");\n}\n";
        let merged = remap(a, b, u).unwrap();
        assert_eq!(
            merged,
            "// Почини код.\nfn main() {\n    // my note\n    let x = 2;\n    println!(\"{x}\");\n}\n"
        );
        assert_eq!(remap(a, b, a).unwrap(), b);
        assert_eq!(remap(b, a, &merged).unwrap(), u);
    }

    #[test]
    fn remap_requires_aligned_variants() {
        assert_eq!(remap("a\nb", "x", "a\nb"), None);
    }

    #[test]
    fn remap_handles_crlf_and_reordering() {
        let a = "// one\n// two\ncode\n";
        let b = "// odin\n// dva\ncode\n";
        let u = "// two\r\n// one\r\ncode\r\n";
        let merged = remap(a, b, u).unwrap();
        let lines = merged.split('\n').collect::<Vec<_>>();
        assert_eq!(lines[2], "code\r");
        assert!(lines[0] == "// dva\r" || lines[1] == "// odin\r");
    }

    #[test]
    fn matching_is_an_lcs() {
        let a = ["x", "a", "b", "c", "y"];
        let b = ["a", "q", "c", "y", "z"];
        assert_eq!(matching_lines(&a, &b), vec![(1, 0), (3, 2), (4, 3)]);
        assert!(matching_lines(&[], &["a"]).is_empty());
    }

    #[test]
    fn decisions() {
        let old = b"// en\ncode\n";
        let new = b"// ru\ncode\n";
        assert!(matches!(
            decide("x.rs", Some(old), new, old),
            Action::Recreate
        ));
        assert!(matches!(decide("x.rs", Some(old), new, new), Action::Skip));
        assert!(matches!(
            decide("README.md", Some(b"a"), b"b", b"mine"),
            Action::Recreate
        ));
        assert!(matches!(
            decide("data.txt", Some(b"a"), b"b", b"mine"),
            Action::Skip
        ));
        let Action::Merge(m) = decide("x.rs", Some(old), new, b"// en\ncode2\n") else {
            panic!("expected a merge");
        };
        assert_eq!(m, "// ru\ncode2\n");
    }
}
