use anyhow::{Context, Result};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn write_file(path: impl AsRef<Path>, content: &[u8]) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create the directory {}", parent.display()))?;
    }
    fs::write(path, content).with_context(|| format!("Failed to write {}", path.display()))
}

pub fn write_if_changed(path: impl AsRef<Path>, content: &[u8]) -> Result<bool> {
    let path = path.as_ref();
    if let Ok(old) = fs::read(path)
        && old == content
    {
        return Ok(false);
    }
    write_file(path, content)?;
    Ok(true)
}

pub fn write_if_missing(path: impl AsRef<Path>, content: &[u8]) -> Result<bool> {
    let path = path.as_ref();
    if path.exists() {
        return Ok(false);
    }
    write_file(path, content)?;
    Ok(true)
}

pub fn remove_dir_if_exists(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("Failed to remove {}", path.display())),
    }
}

pub fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    if from.is_file() {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(from, to)
            .with_context(|| format!("Failed to copy {} to {}", from.display(), to.display()))?;
        return Ok(());
    }
    fs::create_dir_all(to).with_context(|| format!("Failed to create {}", to.display()))?;
    for entry in fs::read_dir(from).with_context(|| format!("Failed to read {}", from.display()))? {
        let entry = entry?;
        let name = entry.file_name();
        if entry.file_type()?.is_dir() {
            if name == "target" {
                continue;
            }
            copy_tree(&entry.path(), &to.join(&name))?;
        } else {
            fs::copy(entry.path(), to.join(&name))
                .with_context(|| format!("Failed to copy {}", entry.path().display()))?;
        }
    }
    Ok(())
}

pub fn move_path(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_tree(from, to)?;
    if from.is_dir() {
        fs::remove_dir_all(from)?;
    } else {
        fs::remove_file(from)?;
    }
    Ok(())
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

pub fn today() -> String {
    let (y, m, d) = civil_from_days(now_secs().div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn day_number(date: &str) -> Option<i64> {
    let mut parts = date.trim().splitn(3, '-');
    let y = parts.next()?.parse().ok()?;
    let m = parts.next()?.parse().ok()?;
    let d = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d))
}

pub fn today_number() -> i64 {
    now_secs().div_euclid(86_400)
}

pub fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let rem = secs.rem_euclid(86_400);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

pub fn new_backup_dir(root: &Path) -> PathBuf {
    let base = format!(".rustlings-backup-{}", timestamp());
    let mut path = root.join(&base);
    let mut n = 2;
    while path.exists() {
        path = root.join(format!("{base}-{n}"));
        n += 1;
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_numbers() {
        assert_eq!(day_number("1970-01-01"), Some(0));
        assert_eq!(day_number("2026-10-20"), Some(20_746));
        assert_eq!(day_number("2000-02-29"), Some(11_016));
        assert_eq!(day_number("2026-13-01"), None);
        assert_eq!(day_number("bad"), None);
        assert_eq!(day_number(&today()), Some(today_number()));
    }

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_746), (2026, 10, 20));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }
}
