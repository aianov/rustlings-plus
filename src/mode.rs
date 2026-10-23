use anyhow::{Context, Result};
use crossterm::{
    QueueableCommand, cursor,
    event::{self, Event, KeyCode, KeyEventKind},
    style::{Attribute, Color, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{
        Clear, ClearType, DisableLineWrap, EnableLineWrap, EnterAlternateScreen,
        LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    },
};
use std::{
    io::{self, StdoutLock, Write},
    ops::Range,
};

use crate::{
    app_state::AppState,
    course::{Course, Kind},
    i18n::{fill, tr},
};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    #[default]
    Story,
    Social,
    Sql,
    Cql,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::Story, Mode::Social, Mode::Sql, Mode::Cql];

    pub fn id(self) -> &'static str {
        match self {
            Mode::Story => "story",
            Mode::Social => "social",
            Mode::Sql => "sql",
            Mode::Cql => "cql",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|m| m.id() == s)
    }

    pub fn title(self) -> &'static str {
        let t = tr();
        match self {
            Mode::Story => t.mode_story,
            Mode::Social => t.mode_social,
            Mode::Sql => t.mode_sql,
            Mode::Cql => t.mode_cql,
        }
    }

    pub fn about(self) -> &'static str {
        let t = tr();
        match self {
            Mode::Story => t.mode_story_about,
            Mode::Social => t.mode_social_about,
            Mode::Sql => t.mode_sql_about,
            Mode::Cql => t.mode_cql_about,
        }
    }

    pub fn includes(self, course: &Course, ind: usize) -> bool {
        let ex = &course.exercises[ind];
        match self {
            Mode::Story => true,
            Mode::Social => ex.kind == Kind::Milestone,
            Mode::Sql | Mode::Cql => {
                ex.kind != Kind::Milestone && db_mode(course, ex.chapter) == Some(self)
            }
        }
    }
}

fn db_mode(course: &Course, chapter: usize) -> Option<Mode> {
    let chapter = &course.chapters[chapter];
    if course.tracks[chapter.track].id != "data" {
        return None;
    }
    let id = chapter.id.as_str();
    if id.contains("cql") || id.contains("scylla") {
        Some(Mode::Cql)
    } else if id.contains("sql") || id.contains("pg_") || id.contains("migrations") {
        Some(Mode::Sql)
    } else {
        None
    }
}

pub struct Part {
    pub title: String,
    pub done: usize,
    pub total: usize,
}

impl Part {
    pub fn percent(&self) -> usize {
        (self.done * 100).checked_div(self.total).unwrap_or(100)
    }

    pub fn color(&self) -> Color {
        if self.done == self.total {
            Color::Green
        } else if self.done > 0 {
            Color::Yellow
        } else {
            Color::DarkGrey
        }
    }
}

pub struct Prerequisites {
    pub tracks: Vec<Part>,
    pub topics: Vec<Part>,
}

impl Prerequisites {
    pub fn percent(&self) -> usize {
        let done = self.tracks.iter().map(|p| p.done).sum::<usize>();
        let total = self.tracks.iter().map(|p| p.total).sum::<usize>();
        (done * 100).checked_div(total).unwrap_or(100)
    }

    pub fn all_done(&self) -> bool {
        self.tracks.iter().all(|p| p.done == p.total)
    }
}

fn chapter_number(id: &str) -> &str {
    id.split_once('_').map_or(id, |(n, _)| n)
}

fn part(app: &AppState, title: String, chapters: Range<usize>) -> Part {
    let course = app.course();
    let (mut done, mut total) = (0, 0);
    for c in chapters {
        for i in course.chapters[c].exercises.clone() {
            if course.exercises[i].kind != Kind::Milestone {
                total += 1;
                done += usize::from(app.is_done(i));
            }
        }
    }
    Part { title, done, total }
}

pub fn prerequisites(app: &AppState, ind: usize) -> Prerequisites {
    let course = app.course();
    let locale = app.locale();
    let ex = &course.exercises[ind];
    let chapter = ex.chapter;
    let start = ex
        .journey
        .and_then(|j| {
            course
                .journey_milestones(j)
                .into_iter()
                .flatten()
                .filter(|&m| m < ind)
                .max()
        })
        .map_or(0, |m| course.exercises[m].chapter + 1)
        .min(chapter);

    let topics = (start..=chapter)
        .map(|c| {
            let ch = &course.chapters[c];
            let title = ch.title.get(locale);
            let title = if title.is_empty() {
                ch.id.clone()
            } else {
                format!("{} {title}", chapter_number(&ch.id))
            };
            part(app, title, c..c + 1)
        })
        .filter(|p| p.total > 0)
        .collect();

    let tracks = (0..=ex.track)
        .filter_map(|t| {
            let track = &course.tracks[t];
            let title = track.title.get(locale);
            let title = title
                .split_once(':')
                .map_or(title, |(short, _)| short.trim());
            let (title, end) = if t == ex.track {
                let upto = chapter_number(&course.chapters[chapter].id);
                (fill(tr().prereq_upto, &[&title, &upto]), chapter + 1)
            } else {
                (title.to_owned(), track.chapters.end)
            };
            let p = part(app, title, track.chapters.start..end);
            (p.total > 0).then_some(p)
        })
        .collect();

    Prerequisites { tracks, topics }
}

pub fn bar(part: &Part, width: usize) -> String {
    let filled = (width * part.done).checked_div(part.total).unwrap_or(width);
    let mut s = String::with_capacity(width * 3);
    for i in 0..width {
        s.push(if i < filled { '█' } else { '░' });
    }
    s
}

fn next_ln(stdout: &mut StdoutLock) -> io::Result<()> {
    stdout
        .queue(Clear(ClearType::UntilNewLine))?
        .queue(cursor::MoveToNextLine(1))?;
    Ok(())
}

fn draw(
    app: &AppState,
    stdout: &mut StdoutLock,
    selected: usize,
    startup: bool,
    message: &str,
) -> io::Result<()> {
    let t = tr();
    stdout
        .queue(cursor::MoveTo(0, 0))?
        .queue(Clear(ClearType::All))?;
    stdout.queue(SetAttribute(Attribute::Bold))?;
    write!(stdout, "  {}", t.mode_header)?;
    stdout.queue(ResetColor)?;
    next_ln(stdout)?;
    next_ln(stdout)?;

    for (i, mode) in Mode::ALL.into_iter().enumerate() {
        let (done, total) = app.mode_progress(mode);
        let current = i == selected;
        stdout.write_all(if current { b"  > " } else { b"    " })?;
        if current {
            stdout.queue(SetAttribute(Attribute::Reverse))?;
        }
        stdout.queue(SetAttribute(Attribute::Bold))?;
        if total == 0 {
            stdout.queue(SetForegroundColor(Color::DarkGrey))?;
        }
        write!(stdout, " {}. {:<24}", i + 1, mode.title())?;
        stdout.queue(ResetColor)?;
        stdout.queue(SetAttribute(Attribute::Reset))?;
        if total == 0 {
            stdout.queue(SetForegroundColor(Color::DarkGrey))?;
            write!(stdout, "  ({})", t.coming_soon)?;
            stdout.queue(ResetColor)?;
        } else {
            let p = Part {
                title: String::new(),
                done,
                total,
            };
            stdout.write_all(b"  ")?;
            stdout.queue(SetForegroundColor(p.color()))?;
            write!(stdout, "{} {:>3}%", bar(&p, 12), p.percent())?;
            stdout.queue(ResetColor)?;
            write!(stdout, "  {done}/{total}")?;
        }
        next_ln(stdout)?;
        stdout.queue(SetForegroundColor(Color::DarkGrey))?;
        write!(stdout, "       {}", mode.about())?;
        stdout.queue(ResetColor)?;
        next_ln(stdout)?;
        if mode == Mode::Social
            && let Some(next) = app.next_pending_in_mode(mode)
        {
            let pre = prerequisites(app, next);
            stdout.queue(SetForegroundColor(if pre.all_done() {
                Color::Green
            } else {
                Color::Yellow
            }))?;
            write!(
                stdout,
                "       {}",
                fill(t.mode_next, &[&app.exercise(next).name, &pre.percent()])
            )?;
            stdout.queue(ResetColor)?;
            next_ln(stdout)?;
        }
        next_ln(stdout)?;
    }

    if !message.is_empty() {
        stdout.queue(SetForegroundColor(Color::Yellow))?;
        write!(stdout, "  {message}")?;
        stdout.queue(ResetColor)?;
        next_ln(stdout)?;
        next_ln(stdout)?;
    }
    let back = if startup { t.key_quit } else { t.mode_back };
    write!(stdout, "  {}", fill(t.mode_keys, &[&back]))?;
    next_ln(stdout)?;
    stdout.flush()
}

fn menu(app: &AppState, stdout: &mut StdoutLock, startup: bool) -> Result<Option<Mode>> {
    let n = Mode::ALL.len();
    let mut selected = Mode::ALL.iter().position(|m| *m == app.mode()).unwrap_or(0);
    let mut message = "";
    loop {
        draw(app, stdout, selected, startup, message)?;
        message = "";
        let Event::Key(key) = event::read().context("Failed to read terminal event")? else {
            continue;
        };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        let pick = match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                selected = (selected + n - 1) % n;
                false
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                selected = (selected + 1) % n;
                false
            }
            KeyCode::Char(c @ '1'..='9') if (c as usize - '1' as usize) < n => {
                selected = c as usize - '1' as usize;
                true
            }
            KeyCode::Enter | KeyCode::Char(' ') => true,
            KeyCode::Char('q') | KeyCode::Esc => return Ok(None),
            _ => false,
        };
        if pick {
            let mode = Mode::ALL[selected];
            if app.mode_progress(mode).1 == 0 {
                message = tr().mode_empty;
            } else {
                return Ok(Some(mode));
            }
        }
    }
}

pub fn choose(app: &mut AppState, startup: bool) -> Result<bool> {
    let mut stdout = io::stdout().lock();
    stdout
        .queue(EnterAlternateScreen)?
        .queue(cursor::Hide)?
        .queue(DisableLineWrap)?;
    enable_raw_mode()?;

    let res = menu(app, &mut stdout, startup);

    stdout
        .queue(LeaveAlternateScreen)?
        .queue(cursor::Show)?
        .queue(EnableLineWrap)?
        .flush()?;
    disable_raw_mode()?;
    drop(stdout);

    match res? {
        Some(mode) => {
            app.set_mode(mode)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_ids_roundtrip() {
        for mode in Mode::ALL {
            assert_eq!(Mode::parse(mode.id()), Some(mode));
        }
        assert_eq!(Mode::parse(" SQL "), Some(Mode::Sql));
        assert_eq!(Mode::parse("nope"), None);
    }

    #[test]
    fn part_bar_and_percent() {
        let p = Part {
            title: String::new(),
            done: 1,
            total: 2,
        };
        assert_eq!(bar(&p, 4), "██░░");
        assert_eq!(p.percent(), 50);
    }
}
