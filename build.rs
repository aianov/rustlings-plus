use std::{env, fmt::Write as _, fs, path::PathBuf};

#[path = "src/content_walk.rs"]
mod content_walk;

fn missing_chapters(root: &std::path::Path) {
    let Ok(course) = fs::read_to_string(root.join("course/course.toml")) else {
        println!("cargo:warning=course/course.toml is missing");
        return;
    };
    let table = match course.parse::<toml::Table>() {
        Ok(table) => table,
        Err(e) => {
            println!("cargo:warning=course/course.toml is invalid: {e}");
            return;
        }
    };
    let Some(tracks) = table.get("tracks").and_then(|t| t.as_array()) else {
        return;
    };
    for track in tracks {
        let Some(id) = track.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let chapters = track
            .get("chapters")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|c| c.as_str()).collect::<Vec<_>>())
            .unwrap_or_default();
        let missing = chapters
            .iter()
            .filter(|c| {
                !root
                    .join("exercises")
                    .join(id)
                    .join(c)
                    .join("chapter.toml")
                    .is_file()
            })
            .copied()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            println!(
                "cargo:warning=track `{id}`: {}/{} chapter(s) not written yet, skipped: {}",
                missing.len(),
                chapters.len(),
                missing.join(", "),
            );
        }
    }
}

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    for file in content_walk::TOP_FILES {
        println!("cargo:rerun-if-changed={file}");
    }
    for dir in content_walk::TREE_DIRS
        .iter()
        .chain(content_walk::CONTENT_DIRS.iter())
    {
        println!("cargo:rerun-if-changed={dir}");
    }
    println!("cargo:rerun-if-changed=src/content_walk.rs");

    missing_chapters(&root);

    let files = content_walk::collect(&root).expect("failed to walk the course content");
    let mut code = String::with_capacity(files.len() * 160);
    code.push_str("pub static FILES: &[(&str, &[u8])] = &[\n");
    for (rel, abs) in &files {
        let abs = abs.to_str().expect("non UTF-8 content path");
        writeln!(code, "    ({rel:?}, include_bytes!({abs:?})),").unwrap();
    }
    code.push_str("];\n");

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("embedded_files.rs");
    fs::write(out, code).expect("failed to write embedded_files.rs");
}
