use std::fs;
use std::io;
use std::path::Path;

pub fn ensure_gitignore(root: &Path, entry: &str) -> io::Result<bool> {
    let path = root.join(".gitignore");
    let current = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    if current.lines().any(|line| line.trim() == entry) {
        return Ok(false);
    }
    let mut next = current;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(entry);
    next.push('\n');
    fs::write(path, next)?;
    Ok(true)
}
