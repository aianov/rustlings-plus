use anyhow::{Context, Result, bail};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use crate::{
    app_state::{Extras, STATE_FILE_NAME, parse_state, render_state},
    course::{Course, Kind},
    exercise::{self, Ctx},
    fsutil::{copy_tree, move_path, new_backup_dir, remove_dir_if_exists, today, write_file},
    i18n::{Locale, tr},
    lang::{self, Report},
    layout,
    originals::Originals,
    settings::{SETTINGS_FILE, Settings},
};

pub const RENAMED: [(&str, &str); 9] = [
    ("box1", "smart_pointers1"),
    ("rc1", "smart_pointers2"),
    ("arc1", "smart_pointers3"),
    ("cow1", "smart_pointers4"),
    ("using_as", "conversions1"),
    ("from_into", "conversions2"),
    ("from_str", "conversions3"),
    ("try_from_into", "conversions4"),
    ("as_ref_mut", "conversions5"),
];
const MIN_SIMILARITY: f64 = 0.5;

pub struct Migration {
    pub backup: PathBuf,
    pub done: usize,
    pub current: String,
    pub lang: Report,
    pub replaced: Vec<String>,
}

fn new_name(old: &str) -> &str {
    RENAMED
        .iter()
        .find(|(from, _)| *from == old)
        .map_or(old, |(_, to)| to)
}

pub fn similarity(original: &str, learner: &str) -> f64 {
    let meaningful = |text: &'_ str| {
        text.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let (a, u) = (meaningful(original), meaningful(learner));
    let a = a.iter().map(String::as_str).collect::<Vec<_>>();
    let u = u.iter().map(String::as_str).collect::<Vec<_>>();
    if a.is_empty() {
        return 1.0;
    }
    lang::matching_lines(&a, &u).len() as f64 / a.len() as f64
}

pub fn is_old_layout(root: &Path) -> bool {
    if root.join(SETTINGS_FILE).exists() {
        return false;
    }
    if root.join("exercises/00_intro").is_dir() {
        return true;
    }
    fs::read_to_string(root.join("Cargo.toml"))
        .is_ok_and(|manifest| manifest.contains("bin = [") && root.join("exercises").is_dir())
}

fn old_exercise_files(exercises: &Path) -> Result<HashMap<String, PathBuf>> {
    let mut map = HashMap::new();
    let mut visit = |dir: &Path| -> Result<()> {
        for entry in
            fs::read_dir(dir).with_context(|| format!("Failed to read {}", dir.display()))?
        {
            let path = entry?.path();
            if path.is_file()
                && path.extension().is_some_and(|e| e == "rs")
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            {
                map.entry(new_name(stem).to_owned()).or_insert(path);
            }
        }
        Ok(())
    };
    visit(exercises)?;
    for entry in fs::read_dir(exercises)? {
        let path = entry?.path();
        if path.is_dir() {
            visit(&path)?;
        }
    }
    Ok(map)
}

fn ensure_gitignore(root: &Path) -> Result<()> {
    let path = root.join(".gitignore");
    let Ok(mut content) = fs::read_to_string(&path) else {
        return write_file(&path, layout::GITIGNORE);
    };
    if !content.contains(".rustlings-backup-") {
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(".rustlings-backup-*/\n");
        write_file(&path, content.as_bytes())?;
    }
    Ok(())
}

pub fn migrate(course: &'static Course, root: &Path, locale: Locale) -> Result<Migration> {
    if !is_old_layout(root) {
        bail!("{}", tr().not_old_layout);
    }
    let exercises_dir = root.join("exercises");

    let backup = new_backup_dir(root);
    fs::create_dir_all(&backup)
        .with_context(|| format!("Failed to create {}", backup.display()))?;
    for name in [
        "exercises",
        "solutions",
        "Cargo.toml",
        "Cargo.lock",
        STATE_FILE_NAME,
    ] {
        let from = root.join(name);
        if from.exists() {
            copy_tree(&from, &backup.join(name))?;
        }
    }

    let state = fs::read(root.join(STATE_FILE_NAME)).unwrap_or_default();
    let (old_current, old_done) = match parse_state(&state) {
        Some((current, done)) => (
            new_name(&String::from_utf8_lossy(current)).to_owned(),
            done.into_iter()
                .map(|n| new_name(&String::from_utf8_lossy(n)).to_owned())
                .collect::<HashSet<_>>(),
        ),
        None => (String::new(), HashSet::new()),
    };

    let old_files = old_exercise_files(&exercises_dir)?;
    let staging = backup.join(".staging");
    let mut moved = Vec::new();
    for ex in &course.exercises {
        if ex.kind != Kind::File {
            continue;
        }
        if let Some(old) = old_files.get(&ex.name) {
            let staged = staging.join(&ex.src);
            move_path(old, &staged)?;
            moved.push((ex.src.clone(), staged));
        }
    }

    let track_dirs = course
        .tracks
        .iter()
        .map(|t| t.id.clone())
        .collect::<HashSet<_>>();
    for entry in fs::read_dir(&exercises_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if track_dirs.contains(&name) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    remove_dir_if_exists(root.join("solutions"))?;
    let _ = fs::remove_file(root.join("Cargo.toml"));

    for (src, staged) in &moved {
        move_path(staged, &root.join(src))?;
    }
    remove_dir_if_exists(&staging)?;

    let mut lang_report = Report::default();
    let mut replaced = Vec::new();
    for (src, _) in &moved {
        let (Some(base), Some(target)) =
            (course.fs.get(src), course.fs.resolve(src, locale.as_str()))
        else {
            continue;
        };
        let learner = fs::read(root.join(src)).unwrap_or_default();
        if similarity(
            &String::from_utf8_lossy(base),
            &String::from_utf8_lossy(&learner),
        ) < MIN_SIMILARITY
        {
            write_file(root.join(src), target)?;
            if let Some(ex) = course.exercises.iter().find(|e| e.src == *src) {
                replaced.push(ex.name.clone());
            }
            continue;
        }
        if base != target {
            lang::apply(&mut lang_report, &root.join(src), src, Some(base), target)?;
        }
    }

    let mut originals = Originals::load(root);
    for (src, _) in &moved {
        if let Some(target) = course.fs.resolve(src, locale.as_str()) {
            originals.set(src, target);
        }
    }
    originals.save(root)?;

    Settings { locale }.write_to(root)?;
    ensure_gitignore(root)?;
    layout::sync(course, root, locale)?;

    let mut done = vec![false; course.exercises.len()];
    let mut current = 0;
    let mut extras = Extras::default();
    let today = today();
    for (ind, ex) in course.exercises.iter().enumerate() {
        if old_done.contains(&ex.name) && !replaced.contains(&ex.name) {
            done[ind] = true;
            extras.solved.insert(ex.name.clone(), today.clone());
        }
        if ex.name == old_current {
            current = ind;
        }
    }
    write_file(
        root.join(STATE_FILE_NAME),
        &render_state(course, current, &done, &extras),
    )?;

    let ctx = Ctx::new(course, locale, root.to_path_buf());
    for (ind, flag) in done.iter().enumerate() {
        if *flag {
            exercise::write_solution(&ctx, ind)?;
        }
    }

    Ok(Migration {
        backup,
        done: done.iter().filter(|d| **d).count(),
        current: course.exercises[current].name.clone(),
        lang: lang_report,
        replaced,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{course::Course, embedded::ContentFs};

    fn course() -> &'static Course {
        let files: Vec<(&'static str, &'static [u8])> = vec![
            (
                "course/course.toml",
                b"format_version = 2\n[[tracks]]\nid = \"rust-basics\"\nchapters = [\"00_intro\", \"03_if\", \"24_async\"]\n[tracks.dependencies]\n",
            ),
            (
                "exercises/rust-basics/00_intro/chapter.toml",
                b"[chapter]\ntitle = \"Intro\"\n[[exercises]]\nname = \"intro1\"\n[[exercises]]\nname = \"intro2\"\n",
            ),
            ("exercises/rust-basics/00_intro/README.md", b"# Intro\n"),
            ("exercises/rust-basics/00_intro/README.ru.md", b"# Vvedenie\n"),
            (
                "exercises/rust-basics/00_intro/intro1.rs",
                b"// Fix me.\nfn main() {\n    // TODO: print\n}\n",
            ),
            (
                "exercises/rust-basics/00_intro/intro1.ru.rs",
                b"// Pochini.\nfn main() {\n    // TODO: vyvedi\n}\n",
            ),
            ("exercises/rust-basics/00_intro/intro2.rs", b"// two\nfn main() {}\n"),
            (
                "exercises/rust-basics/03_if/chapter.toml",
                b"[chapter]\ntitle = \"If\"\n[[exercises]]\nname = \"quiz1\"\nquiz = true\n",
            ),
            ("exercises/rust-basics/03_if/README.md", b"# If\n"),
            ("exercises/rust-basics/03_if/quiz1.rs", b"// quiz\nfn main() {}\n"),
            (
                "exercises/rust-basics/24_async/chapter.toml",
                b"[chapter]\ntitle = \"Async\"\n[[exercises]]\nname = \"async1\"\n[[exercises]]\nname = \"smart_pointers1\"\n[[exercises]]\nname = \"conversions2\"\n",
            ),
            ("exercises/rust-basics/24_async/README.md", b"# Async\n"),
            ("exercises/rust-basics/24_async/async1.rs", b"fn main() {}\n"),
            (
                "exercises/rust-basics/24_async/smart_pointers1.rs",
                b"// box\nfn main() {\n    let b = 1;\n}\n",
            ),
            (
                "exercises/rust-basics/24_async/conversions2.rs",
                b"// celsius\nstruct Celsius(f64);\nfn main() {}\n",
            ),
            ("solutions/rust-basics/00_intro/intro1.rs", b"// sol\nfn main() {}\n"),
            ("solutions/rust-basics/00_intro/intro1.ru.rs", b"// resh\nfn main() {}\n"),
        ];
        let fs = Box::leak(Box::new(ContentFs::from_entries(&files)));
        Box::leak(Box::new(Course::load(fs).unwrap()))
    }

    #[test]
    fn migrates_old_layout() {
        let course = course();
        let root = std::env::temp_dir().join(format!("rustlings-migrate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let w = |p: &str, c: &str| write_file(root.join(p), c.as_bytes()).unwrap();
        w(
            "Cargo.toml",
            "bin = [\n  { name = \"intro1\", path = \"exercises/00_intro/intro1.rs\" },\n]\n",
        );
        w(
            ".rustlings-state.txt",
            "DON'T EDIT THIS FILE!\n\nquiz1\n\nintro1\nbox1\nfrom_into\n",
        );
        w(
            "exercises/19_smart_pointers/box1.rs",
            "// box\nfn main() {\n    let b = Box::new(1);\n}\n",
        );
        w(
            "exercises/23_conversions/from_into.rs",
            "// person\nstruct Person;\nimpl Default for Person {}\nfn main() {}\n",
        );
        w("exercises/README.md", "old");
        w("exercises/00_intro/README.md", "old intro");
        w(
            "exercises/00_intro/intro1.rs",
            "// Fix me.\nfn main() {\n    // TODO: print\n    println!(\"mine\");\n}\n",
        );
        w("exercises/00_intro/intro2.rs", "// two\nfn main() {}\n");
        w(
            "exercises/quizzes/quiz1.rs",
            "// quiz\nfn main() { let solved = 1; }\n",
        );
        w("solutions/00_intro/intro1.rs", "placeholder");
        w(".gitignore", "target/\n");

        assert!(is_old_layout(&root));
        let m = migrate(course, &root, Locale::Ru).unwrap();
        assert_eq!(m.done, 2);
        assert_eq!(m.replaced, ["conversions2"]);
        assert_eq!(m.current, "quiz1");
        assert!(!is_old_layout(&root));

        let read = |p: &str| fs::read_to_string(root.join(p)).unwrap();
        assert_eq!(
            read("exercises/rust-basics/00_intro/intro1.rs"),
            "// Pochini.\nfn main() {\n    // TODO: vyvedi\n    println!(\"mine\");\n}\n"
        );
        assert_eq!(
            read("exercises/rust-basics/03_if/quiz1.rs"),
            "// quiz\nfn main() { let solved = 1; }\n"
        );
        assert_eq!(
            read("exercises/rust-basics/00_intro/README.md"),
            "# Vvedenie\n"
        );
        assert_eq!(
            read("exercises/rust-basics/24_async/async1.rs"),
            "fn main() {}\n"
        );
        assert_eq!(
            read("solutions/rust-basics/00_intro/intro1.rs"),
            "// resh\nfn main() {}\n"
        );
        assert!(!root.join("exercises/00_intro").exists());
        assert!(!root.join("exercises/quizzes").exists());
        assert!(read("Cargo.toml").contains("members = [\"tracks/*\"]"));
        assert!(read("tracks/rust-basics/Cargo.toml").contains("rust-basics/03_if/quiz1.rs"));
        assert!(read(".rustlings-state.txt").starts_with(
            "DON'T EDIT THIS FILE!\n\nquiz1\n\nintro1\nsmart_pointers1\n\n@solved intro1 "
        ));
        assert_eq!(
            read("exercises/rust-basics/24_async/smart_pointers1.rs"),
            "// box\nfn main() {\n    let b = Box::new(1);\n}\n"
        );
        assert_eq!(
            read("exercises/rust-basics/24_async/conversions2.rs"),
            "// celsius\nstruct Celsius(f64);\nfn main() {}\n"
        );
        assert!(read(".gitignore").contains(".rustlings-backup-*/"));
        assert!(read(SETTINGS_FILE).contains("locale = \"ru\""));
        assert_eq!(
            fs::read_to_string(m.backup.join("exercises/quizzes/quiz1.rs")).unwrap(),
            "// quiz\nfn main() { let solved = 1; }\n"
        );
        assert!(m.backup.join("Cargo.toml").is_file());
        assert!(!m.backup.join(".staging").exists());
        assert!(migrate(course, &root, Locale::Ru).is_err());
        fs::remove_dir_all(&root).unwrap();
    }
}
