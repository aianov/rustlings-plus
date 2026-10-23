use anyhow::{Context, Result};
use std::{env, fs, path::Path, sync::OnceLock};

use crate::content_walk;

mod files {
    include!(concat!(env!("OUT_DIR"), "/embedded_files.rs"));
}

pub const CONTENT_DIR_ENV: &str = "RUSTLINGS_CONTENT_DIR";
pub const LOCALES: [&str; 2] = ["en", "ru"];

type Entry = (&'static str, &'static [u8]);

pub struct ContentFs {
    files: &'static [Entry],
}

static FS: OnceLock<ContentFs> = OnceLock::new();

fn load_dir(root: &Path) -> Result<&'static [Entry]> {
    let collected = content_walk::collect(root)
        .with_context(|| format!("Failed to read the course content in {}", root.display()))?;
    let mut entries = Vec::with_capacity(collected.len());
    for (rel, abs) in collected {
        let content =
            fs::read(&abs).with_context(|| format!("Failed to read {}", abs.display()))?;
        entries.push((&*rel.leak(), &*content.leak()));
    }
    Ok(entries.leak())
}

pub fn init() -> Result<&'static ContentFs> {
    if let Some(fs) = FS.get() {
        return Ok(fs);
    }
    let files = match env::var_os(CONTENT_DIR_ENV) {
        Some(dir) if !dir.is_empty() => load_dir(Path::new(&dir))?,
        _ => files::FILES,
    };
    Ok(FS.get_or_init(|| ContentFs { files }))
}

impl ContentFs {
    #[cfg(test)]
    pub fn from_entries(files: &[Entry]) -> Self {
        let mut files = files.to_vec();
        files.sort_unstable();
        Self {
            files: files.leak(),
        }
    }

    pub fn get(&self, path: &str) -> Option<&'static [u8]> {
        self.files
            .binary_search_by(|(p, _)| (*p).cmp(path))
            .ok()
            .map(|ind| self.files[ind].1)
    }

    pub fn exists(&self, path: &str) -> bool {
        self.get(path).is_some()
    }

    pub fn under(&self, dir: &str) -> &'static [Entry] {
        let prefix = format!("{dir}/");
        let start = self.files.partition_point(|(p, _)| *p < prefix.as_str());
        let files = &self.files[start..];
        let len = files.partition_point(|(p, _)| p.starts_with(&prefix));
        &files[..len]
    }

    #[cfg(test)]
    pub fn has_dir(&self, dir: &str) -> bool {
        !self.under(dir).is_empty()
    }

    pub fn resolve(&self, path: &str, locale: &str) -> Option<&'static [u8]> {
        if let Some(variant) = variant_path(path, locale)
            && let Some(content) = self.get(&variant)
        {
            return Some(content);
        }
        self.get(path)
    }

    pub fn tree(&self, dir: &str, locale: &str) -> Vec<(String, &'static [u8])> {
        let mut out: Vec<(String, &'static [u8])> = Vec::new();
        for (path, content) in self.under(dir) {
            let rel = &path[dir.len() + 1..];
            let (parent, name) = match rel.rfind('/') {
                Some(ind) => (&rel[..=ind], &rel[ind + 1..]),
                None => ("", rel),
            };
            if let Some((stem, loc, ext)) = split_variant(name) {
                if loc == locale {
                    out.push((format!("{parent}{stem}{ext}"), content));
                }
                continue;
            }
            if let Some(variant) = variant_path(path, locale)
                && self.exists(&variant)
            {
                continue;
            }
            out.push((rel.to_owned(), content));
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
}

pub fn split_variant(name: &str) -> Option<(&str, &str, &str)> {
    let ext_dot = name.rfind('.')?;
    let ext = &name[ext_dot..];
    if ext.len() < 2 {
        return None;
    }
    let rest = &name[..ext_dot];
    let loc_dot = rest.rfind('.')?;
    let loc = &rest[loc_dot + 1..];
    let stem = &rest[..loc_dot];
    if stem.is_empty() || !LOCALES.contains(&loc) {
        return None;
    }
    Some((stem, loc, ext))
}

pub fn variant_path(path: &str, locale: &str) -> Option<String> {
    let name_start = path.rfind('/').map_or(0, |ind| ind + 1);
    let name = &path[name_start..];
    let dot = name.rfind('.').filter(|&ind| ind > 0)?;
    let (stem, ext) = name.split_at(dot);
    Some(format!("{}{stem}.{locale}{ext}", &path[..name_start]))
}

#[cfg(test)]
mod tests {
    use super::*;

    static ENTRIES: &[Entry] = &[
        ("a/b/README.md", b"base readme"),
        ("a/b/README.ru.md", b"ru readme"),
        ("a/b/x.rs", b"x base"),
        ("a/b/x.ru.rs", b"x ru"),
        ("a/b/y.rs", b"y base"),
        ("a/b/z.en.rs", b"z en only"),
        ("a/b/sub/Cargo.toml", b"cargo"),
        ("a/b/sub/Cargo.ru.toml", b"cargo ru"),
        ("a/bc/other.rs", b"other"),
    ];

    #[test]
    fn variants() {
        assert_eq!(split_variant("x.ru.rs"), Some(("x", "ru", ".rs")));
        assert_eq!(split_variant("README.en.md"), Some(("README", "en", ".md")));
        assert_eq!(split_variant("a.b.ru.rs"), Some(("a.b", "ru", ".rs")));
        assert_eq!(split_variant("x.rs"), None);
        assert_eq!(split_variant("x.de.rs"), None);
        assert_eq!(split_variant(".ru.rs"), None);
        assert_eq!(split_variant("Makefile.ru"), None);
        assert_eq!(variant_path("d/x.rs", "ru").as_deref(), Some("d/x.ru.rs"));
        assert_eq!(
            variant_path("x.tar.gz", "en").as_deref(),
            Some("x.tar.en.gz")
        );
        assert_eq!(variant_path("d/Makefile", "ru"), None);
        assert_eq!(variant_path("d/.gitignore", "ru"), None);
    }

    #[test]
    fn under_and_resolve() {
        let fs = ContentFs::from_entries(ENTRIES);
        assert_eq!(fs.under("a/b").len(), 8);
        assert_eq!(fs.under("a/bc").len(), 1);
        assert_eq!(fs.under("a").len(), 9);
        assert!(fs.under("a/b/x").is_empty());
        assert!(fs.has_dir("a/b/sub"));
        assert_eq!(fs.resolve("a/b/x.rs", "ru"), Some(&b"x ru"[..]));
        assert_eq!(fs.resolve("a/b/x.rs", "en"), Some(&b"x base"[..]));
        assert_eq!(fs.resolve("a/b/y.rs", "ru"), Some(&b"y base"[..]));
        assert_eq!(fs.resolve("a/b/missing.rs", "ru"), None);
    }

    #[test]
    fn tree_applies_variant_rule() {
        let fs = ContentFs::from_entries(ENTRIES);
        let names = |loc| {
            fs.tree("a/b", loc)
                .into_iter()
                .map(|(p, c)| (p, String::from_utf8(c.to_vec()).unwrap()))
                .collect::<Vec<_>>()
        };
        let pair = |a: &str, b: &str| (a.to_owned(), b.to_owned());
        assert_eq!(
            names("ru"),
            vec![
                pair("README.md", "ru readme"),
                pair("sub/Cargo.toml", "cargo ru"),
                pair("x.rs", "x ru"),
                pair("y.rs", "y base"),
            ],
        );
        assert_eq!(
            names("en"),
            vec![
                pair("README.md", "base readme"),
                pair("sub/Cargo.toml", "cargo"),
                pair("x.rs", "x base"),
                pair("y.rs", "y base"),
                pair("z.rs", "z en only"),
            ],
        );
    }
}
