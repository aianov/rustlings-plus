use anyhow::{Context, Error, Result, bail};
use crossterm::{
    QueueableCommand, cursor,
    style::{Color, ResetColor, SetForegroundColor},
};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{File, OpenOptions},
    io::{Read, Seek, StdoutLock, Write},
    ops::Range,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering::Relaxed},
        mpsc,
    },
    thread,
};

use crate::{
    clear_terminal,
    course::{Course, Exercise, Kind},
    editor::{Editor, EditorJoinHandle},
    exercise::{self, Ctx, Outcome},
    fsutil::{today, write_file},
    i18n::{self, Locale, fill, tr},
    journey, lang,
    mode::Mode,
    term::CheckProgressVisualizer,
};

pub const STATE_FILE_NAME: &str = ".rustlings-state.txt";
const DEFAULT_CHECK_PARALLELISM: usize = 8;
const STATE_FILE_HEADER: &[u8] = b"DON'T EDIT THIS FILE!\n\n";

#[must_use]
pub enum ExercisesProgress {
    AllDone,
    NewPending,
    CurrentPending,
}

pub enum StateFileStatus {
    Read,
    NotRead,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Done,
    Started,
    Untouched,
}

impl Status {
    pub fn color(self) -> Color {
        match self {
            Status::Done => Color::Green,
            Status::Started => Color::Red,
            Status::Untouched => Color::Grey,
        }
    }
}

pub fn parse_state(buf: &[u8]) -> Option<(&[u8], HashSet<&[u8]>)> {
    let mut lines = buf.split(|c| *c == b'\n').skip(2);
    let current = lines.next()?;
    if current.is_empty() || lines.next().is_none() {
        return None;
    }
    let mut done = HashSet::new();
    for name in lines {
        if name.is_empty() {
            break;
        }
        done.insert(name);
    }
    Some((current, done))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Review {
    pub date: String,
    pub names: Vec<String>,
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct Extras {
    pub hints: BTreeMap<String, usize>,
    pub solved: BTreeMap<String, String>,
    pub answers: BTreeMap<String, String>,
    pub review: Option<Review>,
    pub last_review: Option<String>,
    pub mode: Option<String>,
    pub positions: BTreeMap<String, String>,
}

impl Extras {
    pub fn parse(buf: &[u8]) -> Self {
        let mut extras = Self::default();
        for line in String::from_utf8_lossy(buf).lines() {
            let Some(line) = line.strip_prefix('@') else {
                continue;
            };
            let (key, rest) = line.split_once(' ').unwrap_or((line, ""));
            let (first, second) = rest.split_once(' ').unwrap_or((rest, ""));
            match key {
                "hints" => {
                    if let Ok(n) = second.trim().parse() {
                        extras.hints.insert(first.to_owned(), n);
                    }
                }
                "solved" => {
                    extras
                        .solved
                        .insert(first.to_owned(), second.trim().to_owned());
                }
                "answer" => {
                    extras.answers.insert(first.to_owned(), second.to_owned());
                }
                "review" => {
                    extras.review = Some(Review {
                        date: first.to_owned(),
                        names: second
                            .split(',')
                            .map(str::trim)
                            .filter(|n| !n.is_empty())
                            .map(str::to_owned)
                            .collect(),
                    });
                }
                "reviewed" => extras.last_review = Some(first.trim().to_owned()),
                "mode" => extras.mode = Some(first.trim().to_owned()),
                "at" => {
                    extras
                        .positions
                        .insert(first.to_owned(), second.trim().to_owned());
                }
                _ => (),
            }
        }
        extras
    }

    fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for (name, n) in &self.hints {
            lines.push(format!("@hints {name} {n}"));
        }
        for (name, date) in &self.solved {
            lines.push(format!("@solved {name} {date}"));
        }
        for (name, answer) in &self.answers {
            let answer = answer.replace(['\n', '\r'], " ");
            lines.push(format!("@answer {name} {answer}"));
        }
        if let Some(review) = &self.review {
            lines.push(format!(
                "@review {} {}",
                review.date,
                review.names.join(",")
            ));
        }
        if let Some(date) = &self.last_review {
            lines.push(format!("@reviewed {date}"));
        }
        if let Some(mode) = &self.mode {
            lines.push(format!("@mode {mode}"));
        }
        for (mode, name) in &self.positions {
            lines.push(format!("@at {mode} {name}"));
        }
        lines
    }
}

pub fn render_state(course: &Course, current: usize, done: &[bool], extras: &Extras) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4096);
    buf.extend_from_slice(STATE_FILE_HEADER);
    buf.extend_from_slice(course.exercises[current].name.as_bytes());
    buf.push(b'\n');
    for (ex, done) in course.exercises.iter().zip(done) {
        if *done {
            buf.push(b'\n');
            buf.extend_from_slice(ex.name.as_bytes());
        }
    }
    let lines = extras.lines();
    if !lines.is_empty() {
        buf.extend_from_slice(b"\n\n");
        buf.extend_from_slice(lines.join("\n").as_bytes());
        buf.push(b'\n');
    }
    buf
}

pub struct AppState {
    ctx: Ctx,
    current: usize,
    done: Vec<bool>,
    n_done: usize,
    state_file: File,
    emit_file_links: bool,
    editor: Option<Editor>,
    notice: Option<String>,
    extras: Extras,
    outdated: Vec<usize>,
    mode: Mode,
}

impl AppState {
    pub fn new(
        course: &'static Course,
        locale: Locale,
        root: PathBuf,
        editor: Option<Editor>,
        vs_code_term: bool,
    ) -> Result<(Self, StateFileStatus)> {
        let mut state_file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(root.join(STATE_FILE_NAME))
            .with_context(|| {
                format!("Failed to open or create the state file {STATE_FILE_NAME}")
            })?;

        let mut done = vec![false; course.exercises.len()];
        let mut current = 0;
        let mut n_done = 0;
        let mut buf = Vec::with_capacity(2048);
        let read_ok = state_file.read_to_end(&mut buf).is_ok();
        let extras = Extras::parse(&buf);
        let status = if read_ok && let Some((current_name, done_names)) = parse_state(&buf) {
            for (ind, ex) in course.exercises.iter().enumerate() {
                if done_names.contains(ex.name.as_bytes()) {
                    done[ind] = true;
                    n_done += 1;
                }
                if ex.name.as_bytes() == current_name {
                    current = ind;
                }
            }
            StateFileStatus::Read
        } else {
            StateFileStatus::NotRead
        };

        let mode = extras
            .mode
            .as_deref()
            .and_then(Mode::parse)
            .unwrap_or_default();
        let slf = Self {
            ctx: Ctx::new(course, locale, root),
            current,
            done,
            n_done,
            state_file,
            emit_file_links: !vs_code_term,
            editor,
            notice: None,
            extras,
            outdated: Vec::new(),
            mode,
        };
        Ok((slf, status))
    }

    pub fn ctx(&self) -> &Ctx {
        &self.ctx
    }

    pub fn course(&self) -> &'static Course {
        self.ctx.course
    }

    pub fn locale(&self) -> Locale {
        self.ctx.locale
    }

    pub fn current_exercise_ind(&self) -> usize {
        self.current
    }

    pub fn current_exercise(&self) -> &'static Exercise {
        &self.ctx.course.exercises[self.current]
    }

    pub fn exercise(&self, ind: usize) -> &'static Exercise {
        &self.ctx.course.exercises[ind]
    }

    pub fn len(&self) -> usize {
        self.done.len()
    }

    pub fn is_done(&self, ind: usize) -> bool {
        self.done[ind]
    }

    pub fn n_done(&self) -> usize {
        self.n_done
    }

    pub fn n_pending(&self) -> usize {
        self.done.len() - self.n_done
    }

    pub fn emit_file_links(&self) -> bool {
        self.emit_file_links
    }

    pub fn set_outdated(&mut self, outdated: Vec<usize>) {
        self.outdated = outdated;
    }

    pub fn is_outdated(&self, ind: usize) -> bool {
        self.outdated.contains(&ind)
    }

    pub fn extras(&self) -> &Extras {
        &self.extras
    }

    pub fn update_extras(&mut self, f: impl FnOnce(&mut Extras)) -> Result<()> {
        f(&mut self.extras);
        self.write()
    }

    pub fn hints_opened(&self, ind: usize) -> usize {
        let ex = self.exercise(ind);
        let total = ex.hints(self.ctx.locale).len();
        self.extras
            .hints
            .get(&ex.name)
            .copied()
            .unwrap_or(0)
            .min(total)
    }

    pub fn open_next_hint(&mut self, ind: usize) -> Result<usize> {
        let ex = self.exercise(ind);
        let total = ex.hints(self.ctx.locale).len();
        let opened = (self.hints_opened(ind) + 1).min(total);
        if opened > 0 {
            let name = ex.name.clone();
            self.update_extras(|e| {
                e.hints.insert(name, opened);
            })?;
        }
        Ok(opened)
    }

    pub fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn in_mode(&self, ind: usize) -> bool {
        self.mode.includes(self.ctx.course, ind)
    }

    pub fn mode_progress(&self, mode: Mode) -> (usize, usize) {
        let course = self.ctx.course;
        let (mut done, mut total) = (0, 0);
        for (ind, d) in self.done.iter().enumerate() {
            if mode.includes(course, ind) {
                total += 1;
                done += usize::from(*d);
            }
        }
        (done, total)
    }

    pub fn mode_status(&self) -> Status {
        let (done, total) = self.mode_progress(self.mode);
        if total > 0 && done == total {
            Status::Done
        } else if done > 0 || self.in_mode(self.current) {
            Status::Started
        } else {
            Status::Untouched
        }
    }

    pub fn next_pending_in_mode(&self, mode: Mode) -> Option<usize> {
        let course = self.ctx.course;
        (0..self.done.len()).find(|&i| !self.done[i] && mode.includes(course, i))
    }

    pub fn set_mode(&mut self, mode: Mode) -> Result<()> {
        let course = self.ctx.course;
        let current = course.exercises[self.current].name.clone();
        self.extras
            .positions
            .insert(self.mode.id().to_owned(), current);
        self.mode = mode;
        self.extras.mode = Some(mode.id().to_owned());
        let target = self
            .extras
            .positions
            .get(mode.id())
            .and_then(|name| course.find(name))
            .filter(|&i| self.in_mode(i))
            .or_else(|| self.in_mode(self.current).then_some(self.current))
            .or_else(|| self.next_pending_in_mode(mode))
            .or_else(|| (0..self.done.len()).find(|&i| self.in_mode(i)));
        self.write()?;
        match target {
            Some(ind) => self.set_current_exercise_ind(ind),
            None => Ok(()),
        }
    }

    pub fn range_progress(&self, range: Range<usize>) -> (usize, usize) {
        let total = range.len();
        (self.done[range].iter().filter(|d| **d).count(), total)
    }

    pub fn range_status(&self, range: Range<usize>) -> Status {
        let (done, total) = self.range_progress(range.clone());
        if total > 0 && done == total {
            Status::Done
        } else if done > 0 || range.contains(&self.current) {
            Status::Started
        } else {
            Status::Untouched
        }
    }

    pub fn track_range(&self, track: usize) -> Range<usize> {
        self.ctx.course.track_exercises(track)
    }

    pub fn chapter_range(&self, chapter: usize) -> Range<usize> {
        self.ctx.course.chapters[chapter].exercises.clone()
    }

    pub fn exercise_status(&self, ind: usize) -> Status {
        if self.done[ind] {
            Status::Done
        } else if ind == self.current {
            Status::Started
        } else {
            Status::Untouched
        }
    }

    pub fn journey_progress(&self, journey: usize) -> (usize, usize, Status) {
        let course = self.ctx.course;
        let milestones = course.journey_milestones(journey);
        let total = milestones.len();
        let done = milestones
            .iter()
            .filter(|m| m.is_some_and(|i| self.done[i]))
            .count();
        let current_in = milestones.contains(&Some(self.current));
        let status = if total > 0 && done == total {
            Status::Done
        } else if done > 0 || current_in {
            Status::Started
        } else {
            Status::Untouched
        };
        (done, total, status)
    }

    fn write(&mut self) -> Result<()> {
        let buf = render_state(self.ctx.course, self.current, &self.done, &self.extras);
        self.state_file
            .rewind()
            .with_context(|| format!("Failed to rewind the state file {STATE_FILE_NAME}"))?;
        self.state_file
            .set_len(0)
            .with_context(|| format!("Failed to truncate the state file {STATE_FILE_NAME}"))?;
        self.state_file
            .write_all(&buf)
            .with_context(|| format!("Failed to write the state file {STATE_FILE_NAME}"))
    }

    pub fn save(&mut self) -> Result<()> {
        self.write()
    }

    fn on_open(&mut self, cli: bool) -> Result<()> {
        self.notice = journey::open(&self.ctx, &self.done, self.current, cli)?;
        Ok(())
    }

    pub fn open_current(&mut self, cli: bool) -> Result<()> {
        self.on_open(cli)
    }

    pub fn set_current_exercise_ind(&mut self, ind: usize) -> Result<()> {
        if ind >= self.done.len() {
            bail!("The exercise index is higher than the number of exercises");
        }
        if ind == self.current {
            return Ok(());
        }
        self.current = ind;
        self.write()?;
        self.on_open(false)
    }

    pub fn set_current_exercise_by_name(&mut self, name: &str) -> Result<()> {
        let ind = self
            .ctx
            .course
            .find(name)
            .with_context(|| fill(tr().no_exercise, &[&name]))?;
        self.current = ind;
        self.write()
    }

    pub fn set_status(&mut self, ind: usize, done: bool) -> Result<bool> {
        let flag = self
            .done
            .get_mut(ind)
            .context("The exercise index is higher than the number of exercises")?;
        if *flag == done {
            return Ok(false);
        }
        *flag = done;
        let name = self.ctx.course.exercises[ind].name.clone();
        if done {
            self.n_done += 1;
            self.extras.solved.insert(name, today());
        } else {
            self.n_done -= 1;
            self.extras.solved.remove(&name);
        }
        Ok(true)
    }

    pub fn set_pending(&mut self, ind: usize) -> Result<()> {
        if self.set_status(ind, false)? {
            self.write()?;
        }
        Ok(())
    }

    pub fn reset_exercise(&mut self, ind: usize) -> Result<Option<String>> {
        self.set_pending(ind)?;
        self.outdated.retain(|i| *i != ind);
        let ctx = &self.ctx;
        let ex = &ctx.course.exercises[ind];
        match ex.kind {
            Kind::File => {
                let content = ctx
                    .course
                    .fs
                    .resolve(&ex.src, ctx.loc())
                    .with_context(|| format!("{} is missing in the course", ex.src))?;
                write_file(ctx.root.join(&ex.src), content)?;
                crate::originals::record(&ctx.root, [(ex.src.as_str(), content)])?;
                Ok(None)
            }
            Kind::Project => {
                exercise::materialize_project(ctx, ex)?;
                Ok(None)
            }
            Kind::Milestone => journey::baseline(ctx, ind).map(Some),
            Kind::Question => {
                let name = ex.name.clone();
                self.update_extras(|e| {
                    e.answers.remove(&name);
                })?;
                Ok(None)
            }
        }
    }

    pub fn reset_current_exercise(&mut self) -> Result<Option<String>> {
        self.reset_exercise(self.current)
    }

    pub fn baseline(&mut self, ind: usize) -> Result<String> {
        let message = journey::baseline(&self.ctx, ind)?;
        self.notice = None;
        Ok(message)
    }

    pub fn next_pending_exercise_ind(&self) -> Option<usize> {
        let pending = |i: &usize| !self.done[*i] && self.in_mode(*i);
        (self.current + 1..self.done.len())
            .find(pending)
            .or_else(|| (0..self.current).find(pending))
    }

    pub fn first_pending_in(&self, range: Range<usize>) -> Option<usize> {
        let start = range.start;
        self.done[range].iter().position(|d| !d).map(|i| start + i)
    }

    pub fn write_current_solution(&self) -> Result<Option<String>> {
        exercise::write_solution(&self.ctx, self.current)
    }

    pub fn set_locale(&mut self, locale: Locale) -> Result<lang::Report> {
        let report = lang::switch(&self.ctx, &self.done, self.ctx.locale, locale)?;
        self.ctx.locale = locale;
        i18n::set_locale(locale);
        crate::settings::Settings { locale }.write_to(&self.ctx.root)?;
        Ok(report)
    }

    fn check_all_impl(&mut self, stdout: &mut StdoutLock) -> Result<(Option<usize>, Vec<usize>)> {
        let course = self.ctx.course;
        let targets = (0..self.done.len())
            .filter(|&i| course.exercises[i].kind != Kind::Milestone)
            .collect::<Vec<_>>();
        let mut visualizer = CheckProgressVisualizer::build(stdout, targets.len())?;
        let next = &AtomicUsize::new(0);
        let mut outcomes = vec![None; self.done.len()];

        thread::scope(|s| {
            let (sender, receiver) = mpsc::channel();
            let n_threads =
                thread::available_parallelism().map_or(DEFAULT_CHECK_PARALLELISM, |c| c.get());
            for _ in 0..n_threads {
                let sender = sender.clone();
                let ctx = &self.ctx;
                let targets = &targets;
                thread::Builder::new()
                    .spawn_scoped(s, move || {
                        while let Some(&ind) = targets.get(next.fetch_add(1, Relaxed)) {
                            if let Ok(outcome) = exercise::run_exercise(ctx, ind, None)
                                && sender.send((ind, outcome)).is_err()
                            {
                                break;
                            }
                        }
                    })
                    .context("Failed to spawn a thread to check all exercises")?;
            }
            drop(sender);
            while let Ok((ind, outcome)) = receiver.recv() {
                let name = &course.exercises[ind].name;
                match outcome {
                    Outcome::Done => visualizer.done(name)?,
                    Outcome::Pending => visualizer.pending(name)?,
                    Outcome::Skipped => visualizer.skipped(name)?,
                }
                outcomes[ind] = Some(outcome);
            }
            Ok::<_, Error>(())
        })?;
        drop(visualizer);

        let mut first_pending = None;
        let mut skipped = Vec::new();
        for ind in targets {
            let outcome = match outcomes[ind] {
                Some(outcome) => outcome,
                None => exercise::run_exercise(&self.ctx, ind, None)?,
            };
            match outcome {
                Outcome::Done => {
                    self.set_status(ind, true)?;
                }
                Outcome::Pending => {
                    self.set_status(ind, false)?;
                    first_pending.get_or_insert(ind);
                }
                Outcome::Skipped => {
                    skipped.push(ind);
                    if !self.done[ind] {
                        first_pending.get_or_insert(ind);
                    }
                }
            }
        }
        for (ind, ex) in course.exercises.iter().enumerate() {
            if ex.kind == Kind::Milestone {
                if !self.done[ind] {
                    first_pending.get_or_insert(ind);
                }
                skipped.push(ind);
            }
        }
        if first_pending.is_some_and(|f| self.done.iter().take(f).any(|d| !d)) {
            first_pending = self.done.iter().position(|d| !d);
        }
        self.write()?;
        Ok((first_pending, skipped))
    }

    pub fn check_all_exercises(&mut self, stdout: &mut StdoutLock) -> Result<Option<usize>> {
        self.check_all_with_skipped(stdout).map(|(first, _)| first)
    }

    pub fn check_all_with_skipped(
        &mut self,
        stdout: &mut StdoutLock,
    ) -> Result<(Option<usize>, Vec<usize>)> {
        stdout.queue(cursor::Hide)?;
        let res = self.check_all_impl(stdout);
        stdout.queue(cursor::Show)?;
        res
    }

    pub fn mark_current_done(&mut self) -> Result<()> {
        if self.set_status(self.current, true)? {
            self.write()?;
        }
        Ok(())
    }

    pub fn done_current_exercise<const CLEAR_BEFORE_FINAL_CHECK: bool>(
        &mut self,
        stdout: &mut StdoutLock,
    ) -> Result<ExercisesProgress> {
        self.set_status(self.current, true)?;

        if let Some(ind) = self.next_pending_exercise_ind() {
            self.set_current_exercise_ind(ind)?;
            self.write()?;
            return Ok(ExercisesProgress::NewPending);
        }

        if self.mode != Mode::Story {
            self.write()?;
            self.notice = Some(fill(tr().mode_finished, &[&self.mode.title()]));
            return Ok(ExercisesProgress::CurrentPending);
        }

        if CLEAR_BEFORE_FINAL_CHECK {
            clear_terminal(stdout)?;
        } else {
            stdout.write_all(b"\n")?;
        }

        if let Some(first_pending) = self.check_all_exercises(stdout)? {
            self.set_current_exercise_ind(first_pending)?;
            return Ok(ExercisesProgress::NewPending);
        }

        self.write()?;
        self.render_final_message(stdout)?;
        Ok(ExercisesProgress::AllDone)
    }

    pub fn skip_to_next_pending(&mut self) -> Result<bool> {
        match self.next_pending_exercise_ind() {
            Some(ind) => {
                self.set_current_exercise_ind(ind)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn render_final_message(&self, stdout: &mut StdoutLock) -> Result<()> {
        clear_terminal(stdout)?;
        render_finish_line(stdout)?;
        let final_message = self.ctx.course.final_message.get(self.ctx.locale).trim();
        if !final_message.is_empty() {
            stdout.write_all(final_message.as_bytes())?;
            stdout.write_all(b"\n")?;
        }
        Ok(())
    }

    pub fn open_editor(&mut self) -> Result<EditorJoinHandle> {
        if let Some(editor) = self.editor.take() {
            let path = exercise::editor_path(self.ctx.course, self.current_exercise());
            return editor.open(self.current, path);
        }
        Ok(EditorJoinHandle::default())
    }

    pub fn join_editor_handle(&mut self, handle: EditorJoinHandle) -> Result<()> {
        if let Some(editor) = handle.join()? {
            self.editor = Some(editor);
        }
        Ok(())
    }

    pub fn close_editor(&mut self) -> Result<()> {
        if let Some(editor) = &mut self.editor {
            editor.close()?;
        }
        Ok(())
    }
}

fn render_finish_line(stdout: &mut StdoutLock) -> Result<()> {
    let title = tr().finish_line;
    let width = 52usize;
    let len = title.chars().count();
    let left = width.saturating_sub(len) / 2;
    let right = width.saturating_sub(len + left);
    writeln!(stdout, "+{}+", "-".repeat(width))?;
    writeln!(stdout, "|{}{title}{}|", " ".repeat(left), " ".repeat(right))?;
    writeln!(stdout, "+{}  {}+", "-".repeat(25), "-".repeat(25))?;
    writeln!(stdout, "                          \\/")?;
    stdout.queue(SetForegroundColor(Color::Red))?;
    stdout.write_all(FERRIS.as_bytes())?;
    stdout.queue(ResetColor)?;
    stdout.write_all(b"\n")?;
    Ok(())
}

const FERRIS: &str = "    ▒▒          ▒▒▒▒▒▒▒▒      ▒▒▒▒▒▒▒▒          ▒▒
  ▒▒▒▒  ▒▒    ▒▒        ▒▒  ▒▒        ▒▒    ▒▒  ▒▒▒▒
  ▒▒▒▒  ▒▒  ▒▒            ▒▒            ▒▒  ▒▒  ▒▒▒▒
  ▒▒▒▒░░▒▒  ▒▒            ▒▒            ▒▒  ▒▒░░▒▒▒▒
  ▓▓▓▓▓▓▓▓  ▓▓      ▓▓██  ▓▓  ▓▓██      ▓▓  ▓▓▓▓▓▓▓▓
    ▒▒▒▒    ▒▒      ████  ▒▒  ████      ▒▒    ▒▒▒▒
      ▒▒  ▒▒▒▒▒▒        ▒▒▒▒▒▒        ▒▒▒▒▒▒  ▒▒
        ▒▒▒▒▒▒▒▒▒▒▓▓▓▓▓▓▒▒▒▒▒▒▒▒▓▓▓▓▓▓▒▒▒▒▒▒▒▒
          ▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒
            ▒▒▒▒▒▒▒▒▒▒██▒▒▒▒▒▒██▒▒▒▒▒▒▒▒▒▒
          ▒▒  ▒▒▒▒▒▒▒▒▒▒██████▒▒▒▒▒▒▒▒▒▒  ▒▒
        ▒▒    ▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒    ▒▒
      ▒▒    ▒▒    ▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒    ▒▒    ▒▒
      ▒▒  ▒▒    ▒▒                  ▒▒    ▒▒  ▒▒
          ▒▒  ▒▒                      ▒▒  ▒▒
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::course::tests::fixture_course;

    fn app(done: &[bool], current: usize) -> AppState {
        let course = fixture_course();
        let n_done = done.iter().filter(|d| **d).count();
        AppState {
            ctx: Ctx::new(course, Locale::En, std::env::temp_dir()),
            current,
            done: done.to_vec(),
            n_done,
            state_file: tempfile::tempfile().unwrap(),
            emit_file_links: true,
            editor: None,
            notice: None,
            extras: Extras::default(),
            outdated: Vec::new(),
            mode: Mode::Story,
        }
    }

    #[test]
    fn next_pending_wraps_around() {
        let a = app(&[true, false, true, false, true], 1);
        assert_eq!(a.next_pending_exercise_ind(), Some(3));
        let a = app(&[false, true, true, true, true], 4);
        assert_eq!(a.next_pending_exercise_ind(), Some(0));
        let a = app(&[true, true, true, true, true], 2);
        assert_eq!(a.next_pending_exercise_ind(), None);
        let a = app(&[true, true, false, true, true], 2);
        assert_eq!(a.next_pending_exercise_ind(), None);
    }

    #[test]
    fn statuses() {
        let a = app(&[true, true, false, false, false], 2);
        assert_eq!(a.range_status(0..2), Status::Done);
        assert_eq!(a.range_status(a.chapter_range(1)), Status::Started);
        assert_eq!(a.range_status(a.track_range(1)), Status::Untouched);
        assert_eq!(a.range_status(a.track_range(0)), Status::Started);
        assert_eq!(a.exercise_status(0), Status::Done);
        assert_eq!(a.exercise_status(2), Status::Started);
        assert_eq!(a.exercise_status(4), Status::Untouched);
        assert_eq!(a.range_progress(a.track_range(0)), (2, 4));
        assert_eq!(a.journey_progress(0), (0, 2, Status::Untouched));
        let a = app(&[false, false, false, true, false], 0);
        assert_eq!(a.journey_progress(0), (1, 2, Status::Started));
    }

    #[test]
    fn state_roundtrip() {
        let course = fixture_course();
        let buf = render_state(
            course,
            2,
            &[true, false, false, true, false],
            &Extras::default(),
        );
        assert_eq!(buf, b"DON'T EDIT THIS FILE!\n\nproj\n\nintro1\nms_one");
        let (current, done) = parse_state(&buf).unwrap();
        assert_eq!(current, b"proj");
        assert!(done.contains(&b"intro1"[..]) && done.contains(&b"ms_one"[..]));
        assert_eq!(done.len(), 2);
        let buf = render_state(course, 0, &[false; 5], &Extras::default());
        assert_eq!(buf, b"DON'T EDIT THIS FILE!\n\nintro1\n");
        assert!(parse_state(&buf).is_some());
        assert!(parse_state(b"").is_none());

        let mut extras = Extras::default();
        extras.hints.insert("intro1".to_owned(), 2);
        extras
            .solved
            .insert("intro1".to_owned(), "2026-10-20".to_owned());
        extras.answers.insert("q1".to_owned(), "a b\nc".to_owned());
        extras.review = Some(Review {
            date: "2026-10-21".to_owned(),
            names: vec!["intro1".to_owned(), "intro2".to_owned()],
        });
        extras.last_review = Some("2026-10-01".to_owned());
        for done in [[true, false, false, false, false], [false; 5]] {
            let buf = render_state(course, 1, &done, &extras);
            let (current, done_names) = parse_state(&buf).unwrap();
            assert_eq!(current, b"intro2");
            assert_eq!(done_names.len(), usize::from(done[0]));
            let parsed = Extras::parse(&buf);
            assert_eq!(parsed.hints, extras.hints);
            assert_eq!(parsed.solved, extras.solved);
            assert_eq!(parsed.answers["q1"], "a b c");
            assert_eq!(parsed.review, extras.review);
            assert_eq!(parsed.last_review, extras.last_review);
        }
    }
}
