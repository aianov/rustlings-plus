use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub const TOP_FILES: [&str; 2] = ["course/course.toml", "course/docker-compose.yml"];
pub const TREE_DIRS: [&str; 1] = ["course/support"];
pub const CONTENT_DIRS: [&str; 2] = ["exercises", "solutions"];

fn skip_name(name: &str, is_dir: bool) -> bool {
    if is_dir {
        return matches!(name, "target" | ".git" | ".idea" | ".vscode");
    }
    matches!(name, "Cargo.lock" | ".DS_Store") || name.ends_with('~') || name.ends_with(".swp")
}

fn rel_join(prefix: &str, name: &str) -> String {
    let mut s = String::with_capacity(prefix.len() + 1 + name.len());
    s.push_str(prefix);
    s.push('/');
    s.push_str(name);
    s
}

fn walk_tree(dir: &Path, rel: &str, out: &mut Vec<(String, PathBuf)>) -> io::Result<()> {
    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let file_type = entry.file_type()?;
        let is_dir = file_type.is_dir();
        if skip_name(&name, is_dir) {
            continue;
        }
        let path = entry.path();
        let rel = rel_join(rel, &name);
        if is_dir {
            walk_tree(&path, &rel, out)?;
        } else if file_type.is_file() || path.is_file() {
            out.push((rel, path));
        }
    }
    Ok(())
}

fn sub_dirs(dir: &Path) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    let Ok(read) = fs::read_dir(dir) else {
        return Ok(names);
    };
    for entry in read {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        if let Ok(name) = entry.file_name().into_string()
            && !skip_name(&name, true)
            && !name.starts_with('.')
        {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

pub fn collect(root: &Path) -> io::Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::with_capacity(1024);
    for file in TOP_FILES {
        let path = root.join(file);
        if path.is_file() {
            out.push((file.to_owned(), path));
        }
    }
    for dir in TREE_DIRS {
        let path = root.join(dir);
        if path.is_dir() {
            walk_tree(&path, dir, &mut out)?;
        }
    }
    for top in CONTENT_DIRS {
        let top_path = root.join(top);
        for track in sub_dirs(&top_path)? {
            let track_path = top_path.join(&track);
            let track_rel = rel_join(top, &track);
            for chapter in sub_dirs(&track_path)? {
                walk_tree(
                    &track_path.join(&chapter),
                    &rel_join(&track_rel, &chapter),
                    &mut out,
                )?;
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}
