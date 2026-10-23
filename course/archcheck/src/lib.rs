mod comments;
mod glob;
mod regex;

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub rule: String,
    pub path: Option<String>,
    pub message: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Text {
    One(String),
    Many(BTreeMap<String, String>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    kind: String,
    message: Option<Text>,
    paths: Option<Vec<String>>,
    globs: Option<Vec<String>>,
    roots: Option<Vec<String>>,
    members: Option<Vec<String>>,
    packages: Option<Vec<String>>,
    package: Option<String>,
    forbid: Option<Vec<String>>,
    allow_only: Option<Vec<String>>,
    regex: Option<String>,
    max: Option<usize>,
    dirs: Option<Vec<String>>,
    allowed: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Rules {
    #[serde(default)]
    rules: Vec<Rule>,
}

struct Entry {
    rel: String,
    is_dir: bool,
}

struct Tree {
    root: std::path::PathBuf,
    entries: Vec<Entry>,
}

impl Tree {
    fn scan(root: &Path) -> Result<Tree, String> {
        if !root.is_dir() {
            return Err(format!("project dir not found: {}", root.display()));
        }
        let mut entries = Vec::new();
        walk(root, "", &mut entries)?;
        entries.sort_by(|a, b| a.rel.cmp(&b.rel));
        Ok(Tree {
            root: root.to_path_buf(),
            entries,
        })
    }

    fn matching(&self, globs: &[String], dirs: bool) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|e| e.is_dir == dirs && globs.iter().any(|g| glob::matches(g, &e.rel)))
            .collect()
    }

    fn read(&self, rel: &str) -> Option<String> {
        fs::read_to_string(self.root.join(rel)).ok()
    }
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<Entry>) -> Result<(), String> {
    let rd = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for item in rd {
        let item = item.map_err(|e| e.to_string())?;
        let name = item.file_name().to_string_lossy().into_owned();
        let is_dir = item.file_type().map_err(|e| e.to_string())?.is_dir();
        if is_dir && (name == "target" || name.starts_with('.')) {
            continue;
        }
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        out.push(Entry {
            rel: rel.clone(),
            is_dir,
        });
        if is_dir {
            walk(&item.path(), &rel, out)?;
        }
    }
    Ok(())
}

fn localize(text: &Option<Text>, locale: &str) -> Option<String> {
    match text {
        None => None,
        Some(Text::One(s)) => Some(s.clone()),
        Some(Text::Many(m)) => m
            .get(locale)
            .or_else(|| m.get("en"))
            .or_else(|| m.values().next())
            .cloned(),
    }
}

struct Ctx<'a> {
    rule: &'a Rule,
    locale: &'a str,
    out: Vec<Violation>,
}

impl Ctx<'_> {
    fn push(&mut self, path: Option<&str>, detail: Option<String>) {
        let base = localize(&self.rule.message, self.locale)
            .unwrap_or_else(|| format!("rule `{}` violated", self.rule.kind));
        let message = match detail {
            Some(d) => format!("{base} ({d})"),
            None => base,
        };
        self.out.push(Violation {
            rule: self.rule.kind.clone(),
            path: path.map(str::to_string),
            message,
        });
    }

    fn line(&self, n: usize) -> String {
        if self.locale == "ru" {
            format!("строка {n}")
        } else {
            format!("line {n}")
        }
    }
}

fn need<'a, T>(rule: &Rule, field: &'a Option<T>, name: &str) -> Result<&'a T, String> {
    field
        .as_ref()
        .ok_or_else(|| format!("rule `{}`: missing field `{name}`", rule.kind))
}

pub fn check(project_dir: &Path, rules_toml: &str, locale: &str) -> Result<Vec<Violation>, String> {
    let parsed: Rules = toml::from_str(rules_toml).map_err(|e| format!("bad arch.toml: {e}"))?;
    let tree = Tree::scan(project_dir)?;
    let mut all = Vec::new();
    for rule in &parsed.rules {
        let mut cx = Ctx {
            rule,
            locale,
            out: Vec::new(),
        };
        run_rule(&tree, &mut cx)?;
        all.append(&mut cx.out);
    }
    Ok(all)
}

fn run_rule(tree: &Tree, cx: &mut Ctx) -> Result<(), String> {
    let rule = cx.rule;
    match rule.kind.as_str() {
        "exists" => {
            for p in need(rule, &rule.paths, "paths")? {
                let clean = p.trim_start_matches("./").trim_end_matches('/');
                if !tree.root.join(clean).exists() {
                    cx.push(Some(clean), None);
                }
            }
        }
        "absent" => {
            let globs = need(rule, &rule.globs, "globs")?;
            for e in tree
                .entries
                .iter()
                .filter(|e| globs.iter().any(|g| glob::matches(g, &e.rel)))
            {
                cx.push(Some(&e.rel), None);
            }
        }
        "uniform_modules" => {
            for root in need(rule, &rule.roots, "roots")? {
                let root = root.trim_start_matches("./").trim_end_matches('/');
                uniform(tree, root, cx);
            }
        }
        "no_comments" => {
            for e in tree.matching(need(rule, &rule.globs, "globs")?, false) {
                if !e.rel.ends_with(".rs") {
                    continue;
                }
                if let Some(line) = tree
                    .read(&e.rel)
                    .and_then(|t| comments::first_comment_line(&t))
                {
                    let d = cx.line(line);
                    cx.push(Some(&e.rel), Some(d));
                }
            }
        }
        "workspace_members" => workspace_members(tree, cx, need(rule, &rule.members, "members")?),
        "workspace_deps" => {
            for e in tree.matching(need(rule, &rule.packages, "packages")?, true) {
                let manifest = format!("{}/Cargo.toml", e.rel);
                let Some(doc) = tree
                    .read(&manifest)
                    .and_then(|t| toml::from_str::<toml::Table>(&t).ok())
                else {
                    continue;
                };
                for (name, spec) in dep_entries(&doc) {
                    let ok = spec.get("workspace").and_then(|w| w.as_bool()) == Some(true);
                    if !ok {
                        cx.push(Some(&manifest), Some(name));
                    }
                }
            }
        }
        "deps" => {
            let package = need(rule, &rule.package, "package")?;
            let manifest = format!("{}/Cargo.toml", package.trim_end_matches('/'));
            let Some(doc) = tree
                .read(&manifest)
                .and_then(|t| toml::from_str::<toml::Table>(&t).ok())
            else {
                return Ok(());
            };
            let forbid: BTreeSet<&str> = rule.forbid.iter().flatten().map(String::as_str).collect();
            let allow: Option<BTreeSet<&str>> = rule
                .allow_only
                .as_ref()
                .map(|a| a.iter().map(String::as_str).collect());
            if rule.forbid.is_none() && allow.is_none() {
                return Err(format!(
                    "rule `{}`: need `forbid` or `allow_only`",
                    rule.kind
                ));
            }
            for (name, spec) in dep_entries(&doc) {
                let real = spec
                    .get("package")
                    .and_then(|p| p.as_str())
                    .unwrap_or(&name)
                    .to_string();
                let bad = [&name, &real].iter().any(|n| forbid.contains(n.as_str()))
                    || allow
                        .as_ref()
                        .is_some_and(|a| !a.contains(name.as_str()) && !a.contains(real.as_str()));
                if bad {
                    cx.push(Some(&manifest), Some(real));
                }
            }
        }
        "contains" | "not_contains" => {
            let globs = need(rule, &rule.globs, "globs")?;
            let re = regex::Regex::new(need(rule, &rule.regex, "regex")?)?;
            let files = tree.matching(globs, false);
            if rule.kind == "contains" && files.is_empty() {
                cx.push(None, Some(globs.join(", ")));
            }
            for e in files {
                let Some(text) = tree.read(&e.rel) else {
                    continue;
                };
                let found = re.find_line(&text);
                if rule.kind == "contains" && found.is_none() {
                    cx.push(Some(&e.rel), None);
                } else if rule.kind == "not_contains" {
                    if let Some(line) = found {
                        let d = cx.line(line);
                        cx.push(Some(&e.rel), Some(d));
                    }
                }
            }
        }
        "max_lines" => {
            let max = *need(rule, &rule.max, "max")?;
            for e in tree.matching(need(rule, &rule.globs, "globs")?, false) {
                if let Some(text) = tree.read(&e.rel) {
                    let n = text.lines().count();
                    if n > max {
                        cx.push(Some(&e.rel), Some(format!("{n} > {max}")));
                    }
                }
            }
        }
        "allowed_children" => {
            let allowed: BTreeSet<&str> = need(rule, &rule.allowed, "allowed")?
                .iter()
                .map(String::as_str)
                .collect();
            for d in tree.matching(need(rule, &rule.dirs, "dirs")?, true) {
                let prefix = format!("{}/", d.rel);
                for e in &tree.entries {
                    let Some(rest) = e.rel.strip_prefix(&prefix) else {
                        continue;
                    };
                    if !rest.contains('/') && !allowed.contains(rest) {
                        cx.push(Some(&e.rel), None);
                    }
                }
            }
        }
        other => return Err(format!("unknown rule kind `{other}`")),
    }
    Ok(())
}

fn is_special(name: &str) -> bool {
    matches!(name, "mod.rs" | "lib.rs" | "main.rs")
}

fn uniform(tree: &Tree, dir: &str, cx: &mut Ctx) {
    let prefix = if dir.is_empty() {
        String::new()
    } else {
        format!("{dir}/")
    };
    let under = |e: &&Entry| {
        e.rel
            .strip_prefix(&prefix)
            .is_some_and(|r| !r.contains('/'))
    };
    let children: Vec<&Entry> = tree.entries.iter().filter(under).collect();
    let has_rs = |d: &Entry| {
        let p = format!("{}/", d.rel);
        tree.entries
            .iter()
            .any(|e| !e.is_dir && e.rel.starts_with(&p) && e.rel.ends_with(".rs"))
    };
    let module_dirs: Vec<&&Entry> = children.iter().filter(|e| e.is_dir && has_rs(e)).collect();
    if !module_dirs.is_empty() {
        for f in children
            .iter()
            .filter(|e| !e.is_dir && e.rel.ends_with(".rs"))
        {
            let name = f.rel.rsplit('/').next().unwrap_or("");
            if !is_special(name) {
                cx.push(Some(&f.rel), None);
            }
        }
    }
    for d in children.iter().filter(|e| e.is_dir) {
        uniform(tree, &d.rel, cx);
    }
}

fn workspace_members(tree: &Tree, cx: &mut Ctx, required: &[String]) {
    let Some(doc) = tree
        .read("Cargo.toml")
        .and_then(|t| toml::from_str::<toml::Table>(&t).ok())
    else {
        cx.push(Some("Cargo.toml"), None);
        return;
    };
    let Some(ws) = doc.get("workspace").and_then(|w| w.as_table()) else {
        cx.push(Some("Cargo.toml"), None);
        return;
    };
    let members: Vec<String> = ws
        .get("members")
        .and_then(|m| m.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    for pattern in required {
        let targets: Vec<&Entry> = tree.matching(std::slice::from_ref(pattern), true);
        if targets.is_empty() {
            cx.push(Some(pattern), None);
        }
        for t in targets {
            if !members.iter().any(|m| glob::matches(m, &t.rel)) {
                cx.push(Some(&t.rel), None);
            }
        }
    }
}

fn dep_entries(doc: &toml::Table) -> Vec<(String, toml::Table)> {
    const KINDS: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];
    let mut tables: Vec<&toml::Table> = KINDS
        .iter()
        .filter_map(|k| doc.get(*k)?.as_table())
        .collect();
    if let Some(targets) = doc.get("target").and_then(|t| t.as_table()) {
        for t in targets.values().filter_map(|t| t.as_table()) {
            tables.extend(KINDS.iter().filter_map(|k| t.get(*k)?.as_table()));
        }
    }
    let mut out = Vec::new();
    for t in tables {
        for (name, spec) in t {
            let table = match spec {
                toml::Value::Table(t) => t.clone(),
                _ => toml::Table::new(),
            };
            out.push((name.clone(), table));
        }
    }
    out
}

#[cfg(test)]
mod tests;
