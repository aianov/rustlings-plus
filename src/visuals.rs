use anyhow::{Context, Result};
use std::{
    path::Path,
    process::{Command, Stdio},
};

use crate::{
    course::Course,
    fsutil::write_if_changed,
    i18n::{Locale, fill, tr},
};

pub fn chapter_visuals(course: &Course, chapter: usize, locale: Locale) -> Vec<(String, String)> {
    let chapter = &course.chapters[chapter];
    chapter
        .visuals
        .iter()
        .map(|v| {
            let title = v.title.get(locale);
            let title = if title.is_empty() {
                v.file.as_str()
            } else {
                title
            };
            (
                title.to_owned(),
                format!("{}/{}", chapter.exercises_dir(), v.file),
            )
        })
        .collect()
}

fn opener() -> Command {
    if cfg!(target_os = "macos") {
        Command::new("open")
    } else if cfg!(windows) {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "start", ""]);
        cmd
    } else {
        Command::new("xdg-open")
    }
}

pub fn open(course: &Course, root: &Path, rel_path: &str, locale: Locale) -> Result<String> {
    let path = root.join(rel_path);
    if !path.is_file()
        && let Some(content) = course.fs.resolve(rel_path, locale.as_str())
    {
        write_if_changed(&path, content)?;
    }
    let status = opener()
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| fill(tr().open_failed, &[&rel_path, &"spawn"]))?;
    if !status.success() {
        anyhow::bail!("{}", fill(tr().open_failed, &[&rel_path, &status]));
    }
    Ok(fill(tr().visual_opened, &[&rel_path]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::course::tests::fixture_course;

    #[test]
    fn visuals_of_chapter() {
        let course = fixture_course();
        let v = chapter_visuals(course, 0, Locale::Ru);
        assert_eq!(
            v,
            vec![(
                "A".to_owned(),
                "exercises/basics/00_intro/visuals/a.html".to_owned()
            )]
        );
        assert!(chapter_visuals(course, 1, Locale::Ru).is_empty());
    }
}
