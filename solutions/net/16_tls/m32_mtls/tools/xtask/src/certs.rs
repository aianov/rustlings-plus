use std::io;
use std::path::Path;

pub const DIR: &str = "certs";

pub fn run(root: &Path, flag: Option<&str>) -> Result<(), String> {
    let (force, keep) = match flag {
        None => (false, false),
        Some("--force") => (true, false),
        Some("--keep") => (false, true),
        Some(other) => return Err(format!("certs понимает --force или --keep, а не {other}")),
    };
    let dir = root.join(DIR);
    match certgen::issue_all(&dir, force) {
        Ok(written) => {
            for path in written {
                let shown = path.strip_prefix(root).unwrap_or(&path);
                println!("выпущен {}", shown.display());
            }
        }
        Err(error) if keep && error.kind() == io::ErrorKind::AlreadyExists => {
            println!("{DIR}/ уже есть, оставляю как есть");
        }
        Err(error) => return Err(error.to_string()),
    }
    let added = certgen::ensure_gitignore(root, &format!("{DIR}/"))
        .map_err(|e| format!("не дописать .gitignore: {e}"))?;
    if added {
        println!("{DIR}/ добавлен в .gitignore: ключи не попадут в репозиторий");
    }
    Ok(())
}
