use anyhow::{Context, Result, bail};
use crossterm::{
    QueueableCommand,
    style::{Attribute, Color, ResetColor, SetAttribute, SetForegroundColor},
};
use std::{
    env::{current_dir, set_current_dir},
    fs::create_dir,
    io::{self, BufRead, IsTerminal, Write},
    path::Path,
    process::{Command, Stdio},
};

use crate::{
    course::Course,
    i18n::{self, Locale, fill, tr},
    layout,
    settings::{SETTINGS_FILE, Settings},
    term::press_enter_prompt,
};

pub fn check_clippy() -> Result<()> {
    if !Command::new("cargo")
        .arg("clippy")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| tr().cargo_missing)?
        .success()
    {
        bail!("{}", tr().clippy_missing);
    }
    Ok(())
}

fn ask_locale(default: Locale) -> Result<Locale> {
    if !io::stdin().is_terminal() {
        return Ok(default);
    }
    let mut stdout = io::stdout().lock();
    stdout.write_all(fill(tr().lang_prompt, &[&default.as_str()]).as_bytes())?;
    stdout.flush()?;
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer)?;
    Ok(Locale::parse(&answer).unwrap_or(default))
}

fn git_init() {
    let Ok(dir) = current_dir() else {
        return;
    };
    let mut dir = dir.as_path();
    loop {
        if dir.join(".git").exists() || dir.join(".jj").exists() {
            return;
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }
    let _ = Command::new("git")
        .arg("init")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

pub fn init(course: &'static Course, lang: Option<Locale>) -> Result<()> {
    let rustlings_dir = Path::new("rustlings");
    if rustlings_dir.exists() {
        bail!("{}", tr().init_dir_exists);
    }
    if Path::new(SETTINGS_FILE).exists()
        || (Path::new("exercises").is_dir() && Path::new("Cargo.toml").is_file())
    {
        bail!("{}", tr().init_in_initialized);
    }
    check_clippy()?;

    let locale = match lang {
        Some(locale) => locale,
        None => ask_locale(course.default_locale)?,
    };
    i18n::set_locale(locale);

    let mut stdout = io::stdout().lock();
    stdout.write_all(tr().init_will_create.as_bytes())?;
    press_enter_prompt(&mut stdout)?;

    create_dir(rustlings_dir).context("Failed to create the `rustlings/` directory")?;
    set_current_dir(rustlings_dir)
        .context("Failed to change the current directory to `rustlings/`")?;
    let root = current_dir().context("Failed to get the current directory")?;

    Settings { locale }.write()?;
    layout::init_layout(course, &root, locale)?;
    git_init();

    stdout.queue(SetForegroundColor(Color::Green))?;
    stdout.write_all(tr().init_done.as_bytes())?;
    stdout.queue(ResetColor)?;
    stdout.write_all(b"\n\n")?;
    stdout.queue(SetAttribute(Attribute::Bold))?;
    stdout.write_all(tr().post_init.as_bytes())?;
    stdout.queue(ResetColor)?;
    Ok(())
}
