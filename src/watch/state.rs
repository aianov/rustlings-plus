use anyhow::{Context, Result};
use crossterm::{
    QueueableCommand,
    style::{
        Attribute, Attributes, Color, ResetColor, SetAttribute, SetAttributes, SetForegroundColor,
    },
    terminal,
};
use std::{
    io::{self, Read, StdoutLock, Write},
    sync::{
        atomic::Ordering::Relaxed,
        mpsc::{Sender, SyncSender, sync_channel},
    },
    thread,
};

use crate::{
    app_state::{AppState, ExercisesProgress},
    clear_terminal,
    course::{Exercise, Kind},
    diff::{self, FileDiff},
    exercise::{self, OUTPUT_CAPACITY, Outcome, path_link, solution_link_line},
    i18n::{Locale, fill, tr},
    journey,
    mode::{self, Mode, Part},
    review,
    term::{MaxLenWriter, progress_bar, with_snake},
    visuals,
    watch::{
        InputPauseGuard, WatchEvent,
        terminal_event::{TEXT_INPUT, terminal_event_handler},
    },
};

const HEADING_ATTRIBUTES: Attributes = Attributes::none()
    .with(Attribute::Bold)
    .with(Attribute::Underlined);

enum DoneStatus {
    DoneWithSolution(String),
    DoneWithoutSolution,
    Pending,
}

pub enum Confirm {
    Reset,
    Baseline,
}

struct DiffState {
    files: Option<Vec<FileDiff>>,
    selected: usize,
}

#[derive(Default)]
struct QuestionState {
    selected: Vec<usize>,
    text: Option<String>,
    feedback: Option<(bool, String)>,
}

pub struct WatchState<'a> {
    app_state: &'a mut AppState,
    output: Vec<u8>,
    show_hint: bool,
    done_status: DoneStatus,
    manual_run: bool,
    term_width: u16,
    terminal_event_unpause_sender: SyncSender<()>,
    notice: Option<String>,
    visual_menu: Vec<(String, String)>,
    diff: Option<DiffState>,
    question: QuestionState,
}

pub fn style_tag(style: Option<&str>) -> Option<&'static str> {
    let t = tr();
    match style? {
        "fix" => Some(t.style_fix),
        "todo" => Some(t.style_todo),
        "scratch" => Some(t.style_scratch),
        "bughunt" => Some(t.style_bughunt),
        _ => None,
    }
}

pub fn exercise_tags(app_state: &AppState, ind: usize) -> Vec<(String, Color)> {
    let ex = app_state.exercise(ind);
    let t = tr();
    let mut tags = Vec::new();
    match ex.kind {
        Kind::Milestone => tags.push((t.tag_milestone.to_owned(), Color::Magenta)),
        Kind::Project => tags.push((t.tag_project.to_owned(), Color::Cyan)),
        Kind::Question => tags.push((t.tag_question.to_owned(), Color::Blue)),
        Kind::File => (),
    }
    if ex.exam {
        tags.push((t.tag_exam.to_owned(), Color::Red));
    }
    if ex.quiz && ex.kind != Kind::Milestone {
        tags.push((t.tag_quiz.to_owned(), Color::Yellow));
    }
    if let Some(tag) = style_tag(ex.style.as_deref()) {
        tags.push((tag.to_owned(), Color::DarkCyan));
    }
    for service in &ex.requires {
        tags.push((format!("[{service}]"), Color::DarkYellow));
    }
    if app_state.is_outdated(ind) {
        tags.push((t.tag_outdated.to_owned(), Color::Yellow));
    }
    tags
}

pub fn write_hints(
    stdout: &mut impl Write,
    ex: &Exercise,
    locale: Locale,
    opened: usize,
) -> io::Result<()> {
    let t = tr();
    let steps = ex.hints(locale);
    if ex.exam {
        stdout.write_all(t.exam_no_hints.as_bytes())?;
        return stdout.write_all(b"\n");
    }
    if steps.is_empty() {
        stdout.write_all(t.no_hint.as_bytes())?;
        return stdout.write_all(b"\n");
    }
    let opened = opened.clamp(1, steps.len());
    for (i, step) in steps.iter().take(opened).enumerate() {
        if i > 0 {
            stdout.write_all(b"\n")?;
        }
        if steps.len() > 1 {
            stdout
                .queue(SetAttributes(HEADING_ATTRIBUTES))?
                .queue(SetForegroundColor(Color::Cyan))?;
            stdout.write_all(fill(t.hint_step, &[&(i + 1), &steps.len()]).as_bytes())?;
            stdout.queue(ResetColor)?;
            stdout.write_all(b"\n")?;
        }
        stdout.write_all(step.trim().as_bytes())?;
        stdout.write_all(b"\n")?;
    }
    Ok(())
}

impl<'a> WatchState<'a> {
    pub fn build(
        app_state: &'a mut AppState,
        watch_event_sender: Sender<WatchEvent>,
        manual_run: bool,
    ) -> Result<Self> {
        let term_width = terminal::size()
            .context("Failed to get the terminal size")?
            .0;

        let (terminal_event_unpause_sender, terminal_event_unpause_receiver) = sync_channel(0);

        thread::Builder::new()
            .spawn(move || {
                terminal_event_handler(
                    watch_event_sender,
                    terminal_event_unpause_receiver,
                    manual_run,
                );
            })
            .context("Failed to spawn a thread to handle terminal events")?;

        let notice = app_state.take_notice();
        Ok(Self {
            app_state,
            output: Vec::with_capacity(OUTPUT_CAPACITY),
            show_hint: false,
            done_status: DoneStatus::Pending,
            manual_run,
            term_width,
            terminal_event_unpause_sender,
            notice,
            visual_menu: Vec::new(),
            diff: None,
            question: QuestionState::default(),
        })
    }

    pub fn app_state(&self) -> &AppState {
        self.app_state
    }

    fn is_question(&self) -> bool {
        self.app_state.current_exercise().kind == Kind::Question
    }

    pub fn run_current_exercise(&mut self, stdout: &mut StdoutLock) -> Result<()> {
        let _input_pause_guard = InputPauseGuard::scoped_pause();

        if let Some(notice) = self.app_state.take_notice() {
            self.notice = Some(notice);
        }
        self.show_hint = false;
        self.diff = None;
        self.question = QuestionState::default();
        TEXT_INPUT.store(false, Relaxed);

        let ind = self.app_state.current_exercise_ind();
        if self.is_question() {
            self.output.clear();
            self.done_status = if self.app_state.is_done(ind) {
                DoneStatus::DoneWithoutSolution
            } else {
                DoneStatus::Pending
            };
            self.render(stdout)?;
            return Ok(());
        }

        stdout.write_all(b"\n\n")?;
        let checking = fill(tr().checking, &[&self.app_state.current_exercise().name]);

        let editor_handle = self.app_state.open_editor()?;
        let ctx = self.app_state.ctx();
        let output = &mut self.output;
        let outcome = with_snake(stdout, &checking, || {
            exercise::run_exercise(ctx, ind, Some(output))
        })??;
        self.output.push(b'\n');

        self.done_status = if outcome == Outcome::Done {
            self.app_state.mark_current_done()?;
            match self.app_state.write_current_solution()? {
                Some(path) => DoneStatus::DoneWithSolution(path),
                None => DoneStatus::DoneWithoutSolution,
            }
        } else {
            if outcome == Outcome::Pending {
                self.app_state.set_pending(ind)?;
            }
            DoneStatus::Pending
        };

        self.app_state.join_editor_handle(editor_handle)?;
        self.render(stdout)?;
        Ok(())
    }

    fn ask_yes_no(&mut self, stdout: &mut StdoutLock, question: &str) -> Result<bool> {
        clear_terminal(stdout)?;
        stdout.write_all(question.as_bytes())?;
        stdout.write_all(b"\n")?;
        stdout.write_all(tr().yes_no.as_bytes())?;
        stdout.flush()?;
        let mut stdin = io::stdin().lock();
        let mut answer = [0];
        loop {
            stdin
                .read_exact(&mut answer)
                .context("Failed to read the user's input")?;
            match answer[0] {
                b'y' | b'Y' => return Ok(true),
                b'n' | b'N' => return Ok(false),
                _ => continue,
            }
        }
    }

    pub fn confirm(&mut self, stdout: &mut StdoutLock, what: Confirm) -> Result<()> {
        let ex = self.app_state.current_exercise();
        let path = exercise::display_path(self.app_state.course(), ex);
        let t = tr();
        let question = match (&what, ex.kind) {
            (Confirm::Baseline, _) | (Confirm::Reset, Kind::Milestone) => {
                fill(t.reset_milestone, &[&path])
            }
            (Confirm::Reset, Kind::Project) => fill(t.reset_project, &[&path]),
            (Confirm::Reset, Kind::File | Kind::Question) => fill(t.reset_file, &[&path]),
        };
        let applicable = !matches!(what, Confirm::Baseline) || ex.kind == Kind::Milestone;
        let mut rerun = false;
        if applicable && self.ask_yes_no(stdout, &question)? {
            let _guard = InputPauseGuard::scoped_pause();
            self.app_state.close_editor()?;
            let ind = self.app_state.current_exercise_ind();
            let message = match what {
                Confirm::Reset => self.app_state.reset_current_exercise(),
                Confirm::Baseline => self.app_state.baseline(ind).map(Some),
            };
            match message {
                Ok(message) => self.notice = message,
                Err(e) => self.notice = Some(format!("{e:#}")),
            }
            rerun = self.manual_run || ex.kind != Kind::File;
        }
        if rerun {
            self.run_current_exercise(stdout)?;
        } else {
            self.render(stdout)?;
        }
        self.terminal_event_unpause_sender.send(())?;
        Ok(())
    }

    pub fn handle_file_change(&mut self, target: usize, stdout: &mut StdoutLock) -> Result<()> {
        if TEXT_INPUT.load(Relaxed) {
            return Ok(());
        }
        let n = self.app_state.len();
        let current = self.app_state.current_exercise_ind();
        let relevant = if target < n {
            target == current
        } else {
            let ex = self.app_state.current_exercise();
            ex.kind == Kind::Milestone && ex.journey == Some(target - n)
        };
        if relevant {
            self.run_current_exercise(stdout)?;
        }
        Ok(())
    }

    pub fn done(&self) -> bool {
        match self.done_status {
            DoneStatus::DoneWithSolution(_) | DoneStatus::DoneWithoutSolution => true,
            DoneStatus::Pending => false,
        }
    }

    pub fn next_exercise(&mut self, stdout: &mut StdoutLock) -> Result<ExercisesProgress> {
        self.notice = None;
        if self.done() {
            let progress = self.app_state.done_current_exercise::<true>(stdout)?;
            self.notice = self.app_state.take_notice();
            return Ok(progress);
        }
        if self.app_state.skip_to_next_pending()? {
            return Ok(ExercisesProgress::NewPending);
        }
        Ok(ExercisesProgress::CurrentPending)
    }

    pub fn switch_language(&mut self, stdout: &mut StdoutLock) -> Result<()> {
        let guard = InputPauseGuard::scoped_pause();
        let new = self.app_state.locale().next();
        let report = self.app_state.set_locale(new)?;
        self.notice = Some(fill(
            tr().lang_switched,
            &[&new.name(), &report.recreated, &report.merged],
        ));
        drop(guard);
        let notice = self.notice.take();
        self.run_current_exercise(stdout)?;
        self.notice = notice;
        self.render(stdout)?;
        Ok(())
    }

    pub fn visuals(&mut self, stdout: &mut StdoutLock) -> Result<()> {
        let chapter = self.app_state.current_exercise().chapter;
        let list =
            visuals::chapter_visuals(self.app_state.course(), chapter, self.app_state.locale());
        match list.len() {
            0 => self.notice = Some(tr().visuals_none.to_owned()),
            1 => self.open_visual(&list[0].1),
            _ => self.visual_menu = list,
        }
        self.render(stdout)?;
        Ok(())
    }

    fn open_visual(&mut self, path: &str) {
        let ctx = self.app_state.ctx();
        self.notice = Some(
            match visuals::open(ctx.course, &ctx.root, path, ctx.locale) {
                Ok(message) => message,
                Err(e) => format!("{e:#}"),
            },
        );
    }

    fn submit_options(&mut self, chosen: Vec<usize>) -> Result<()> {
        let ex = self.app_state.current_exercise();
        let right = ex.question.check_options(&chosen);
        let answer = chosen
            .iter()
            .map(|i| (i + 1).to_string())
            .collect::<Vec<_>>()
            .join(",");
        self.finish_answer(right, answer)
    }

    fn finish_answer(&mut self, right: bool, answer: String) -> Result<()> {
        let ex = self.app_state.current_exercise();
        let name = ex.name.clone();
        self.app_state.update_extras(|e| {
            e.answers.insert(name, answer);
        })?;
        let explain = ex.question.explain.get(self.app_state.locale()).to_owned();
        self.question.selected.clear();
        self.question.feedback = Some((right, explain));
        if right {
            self.app_state.mark_current_done()?;
            self.done_status = DoneStatus::DoneWithoutSolution;
        }
        Ok(())
    }

    pub fn choose(&mut self, ind: usize, stdout: &mut StdoutLock) -> Result<()> {
        if !self.visual_menu.is_empty() {
            if let Some((_, path)) = self.visual_menu.get(ind).cloned() {
                self.visual_menu.clear();
                self.open_visual(&path);
            } else {
                self.visual_menu.clear();
            }
            return Ok(self.render(stdout)?);
        }
        if let Some(diff) = &mut self.diff {
            if let Some(files) = &diff.files
                && ind < files.len()
            {
                diff.selected = ind;
            }
            return Ok(self.render(stdout)?);
        }
        if self.is_question() && !self.done() {
            let ex = self.app_state.current_exercise();
            let n_options = ex.question.options.get(self.app_state.locale()).len();
            if ind >= n_options || ex.question.is_free_text() {
                return Ok(());
            }
            if ex.question.is_multi() {
                if let Some(pos) = self.question.selected.iter().position(|s| *s == ind) {
                    self.question.selected.remove(pos);
                } else {
                    self.question.selected.push(ind);
                }
                self.question.feedback = None;
            } else {
                self.submit_options(vec![ind])?;
            }
            self.render(stdout)?;
        }
        Ok(())
    }

    pub fn enter(&mut self, stdout: &mut StdoutLock) -> Result<()> {
        if !self.is_question() || self.done() {
            return Ok(());
        }
        if let Some(text) = self.question.text.take() {
            TEXT_INPUT.store(false, Relaxed);
            let right = self.app_state.current_exercise().question.check_text(&text);
            self.finish_answer(right, text.trim().to_owned())?;
        } else if self.app_state.current_exercise().question.is_multi()
            && !self.question.selected.is_empty()
        {
            let chosen = self.question.selected.clone();
            self.submit_options(chosen)?;
        } else {
            return Ok(());
        }
        Ok(self.render(stdout)?)
    }

    pub fn start_answer(&mut self, stdout: &mut StdoutLock) -> Result<()> {
        if self.is_question()
            && !self.done()
            && self.app_state.current_exercise().question.is_free_text()
        {
            self.question.text = Some(String::new());
            self.question.feedback = None;
            TEXT_INPUT.store(true, Relaxed);
            self.render(stdout)?;
        }
        Ok(())
    }

    pub fn type_char(&mut self, c: char, stdout: &mut StdoutLock) -> Result<()> {
        if let Some(text) = &mut self.question.text {
            text.push(c);
            self.render(stdout)?;
        }
        Ok(())
    }

    pub fn backspace(&mut self, stdout: &mut StdoutLock) -> Result<()> {
        if let Some(text) = &mut self.question.text {
            text.pop();
            self.render(stdout)?;
        }
        Ok(())
    }

    pub fn escape(&mut self, stdout: &mut StdoutLock) -> Result<()> {
        if self.question.text.take().is_some() {
            TEXT_INPUT.store(false, Relaxed);
        } else if self.diff.take().is_none() && self.visual_menu.is_empty() {
            return Ok(());
        }
        self.visual_menu.clear();
        Ok(self.render(stdout)?)
    }

    pub fn toggle_diff(&mut self, stdout: &mut StdoutLock) -> Result<()> {
        if self.is_question() {
            return Ok(());
        }
        if let Some(diff) = &mut self.diff {
            let n = diff.files.as_ref().map_or(0, Vec::len);
            if n > 1 && diff.selected + 1 < n {
                diff.selected += 1;
            } else {
                self.diff = None;
            }
            return Ok(self.render(stdout)?);
        }
        if !self.done() {
            self.notice = Some(tr().diff_after_solve.to_owned());
            return Ok(self.render(stdout)?);
        }
        let ind = self.app_state.current_exercise_ind();
        let files = diff::exercise_diff(self.app_state.ctx(), ind)?;
        self.diff = Some(DiffState { files, selected: 0 });
        Ok(self.render(stdout)?)
    }

    pub fn close_menu(&mut self, stdout: &mut StdoutLock) -> Result<()> {
        if !self.visual_menu.is_empty() {
            self.visual_menu.clear();
            self.render(stdout)?;
        }
        Ok(())
    }

    pub fn menu_open(&self) -> bool {
        !self.visual_menu.is_empty()
    }

    fn show_prompt(&self, stdout: &mut StdoutLock) -> io::Result<()> {
        let t = tr();
        let ex = self.app_state.current_exercise();
        let mut keys: Vec<(&str, &str)> = Vec::with_capacity(12);
        if self.question.text.is_some() {
            keys.push(("Enter", t.key_submit));
            keys.push(("Esc", t.key_cancel));
        } else {
            keys.push(("n", t.key_next));
            if self.manual_run && ex.kind != Kind::Question {
                keys.push(("r", t.key_run));
            }
            if !ex.exam && (!self.show_hint || self.more_hints()) {
                keys.push(("h", t.key_hint));
            }
            if ex.kind == Kind::Question && !self.done() {
                if ex.question.is_free_text() {
                    keys.push(("a", t.key_answer));
                } else if ex.question.is_multi() {
                    keys.push(("Enter", t.key_submit));
                }
            }
            if self.done() && ex.kind != Kind::Question {
                keys.push(("d", t.key_diff));
            }
            keys.push(("l", t.key_list));
            keys.push(("m", t.key_mode));
            if !self.app_state.course().chapters[ex.chapter]
                .visuals
                .is_empty()
            {
                keys.push(("v", t.key_visuals));
            }
            if ex.kind == Kind::Milestone
                && !journey::is_first(
                    self.app_state.course(),
                    self.app_state.current_exercise_ind(),
                )
            {
                keys.push(("b", t.key_baseline));
            }
            keys.push(("x", t.key_reset));
            keys.push(("t", t.key_lang));
            keys.push(("q", t.key_quit));
        }

        for (i, (key, label)) in keys.iter().enumerate() {
            if i > 0 {
                stdout.write_all(b" / ")?;
            }
            stdout.queue(SetAttribute(Attribute::Bold))?;
            stdout.write_all(key.as_bytes())?;
            stdout.queue(ResetColor)?;
            stdout.write_all(b":")?;
            stdout.write_all(label.as_bytes())?;
        }
        stdout.write_all(b" ? ")?;
        stdout.flush()
    }

    fn more_hints(&self) -> bool {
        let ind = self.app_state.current_exercise_ind();
        let total = self
            .app_state
            .exercise(ind)
            .hints(self.app_state.locale())
            .len();
        self.app_state.hints_opened(ind) < total
    }

    fn render_question(&self, stdout: &mut StdoutLock) -> io::Result<()> {
        let t = tr();
        let locale = self.app_state.locale();
        let ex = self.app_state.current_exercise();
        let q = &ex.question;
        stdout
            .queue(SetAttributes(HEADING_ATTRIBUTES))?
            .queue(SetForegroundColor(Color::Cyan))?;
        stdout.write_all(t.question.as_bytes())?;
        stdout.queue(ResetColor)?;
        stdout.write_all(b"\n")?;
        stdout.write_all(q.text.get(locale).trim().as_bytes())?;
        stdout.write_all(b"\n\n")?;

        let options = q.options.get(locale);
        for (i, option) in options.iter().enumerate() {
            let marked = self.question.selected.contains(&i);
            let mark = if q.is_multi() {
                if marked { "[x] " } else { "[ ] " }
            } else {
                ""
            };
            stdout.queue(SetAttribute(Attribute::Bold))?;
            write!(stdout, "  {}) ", i + 1)?;
            stdout.queue(ResetColor)?;
            writeln!(stdout, "{mark}{}", option.trim())?;
        }
        if !options.is_empty() {
            stdout.write_all(b"\n")?;
        }

        if !self.done() {
            let instruction = if q.is_free_text() {
                t.question_text
            } else if q.is_multi() {
                t.question_multi
            } else {
                t.question_single
            };
            stdout.queue(SetForegroundColor(Color::DarkGrey))?;
            stdout.write_all(instruction.as_bytes())?;
            stdout.queue(ResetColor)?;
            stdout.write_all(b"\n")?;
        }
        if let Some(text) = &self.question.text {
            stdout.queue(SetAttribute(Attribute::Bold))?;
            stdout.write_all(t.your_answer.as_bytes())?;
            stdout.queue(ResetColor)?;
            writeln!(stdout, "{text}_")?;
        }

        let explain = q.explain.get(locale).trim();
        match &self.question.feedback {
            Some((right, explain)) => {
                stdout.write_all(b"\n")?;
                stdout
                    .queue(SetAttribute(Attribute::Bold))?
                    .queue(SetForegroundColor(if *right {
                        Color::Green
                    } else {
                        Color::Red
                    }))?;
                stdout.write_all(
                    if *right {
                        t.answer_right
                    } else {
                        t.answer_wrong
                    }
                    .as_bytes(),
                )?;
                stdout.queue(ResetColor)?;
                stdout.write_all(b"\n")?;
                if !explain.trim().is_empty() {
                    writeln!(stdout, "{}", explain.trim())?;
                }
            }
            None if self.done() && !explain.is_empty() => {
                writeln!(stdout, "\n{explain}")?;
            }
            None => (),
        }
        stdout.write_all(b"\n")
    }

    fn render_location(&self, stdout: &mut StdoutLock) -> io::Result<()> {
        let t = tr();
        let app = &self.app_state;
        let course = app.course();
        let locale = app.locale();
        let ind = app.current_exercise_ind();
        let ex = app.current_exercise();
        let track = &course.tracks[ex.track];
        let chapter = &course.chapters[ex.chapter];
        let links = app.emit_file_links();

        let mode = app.mode();
        let (label, (done, total), status) = if mode == Mode::Story {
            let track_range = app.track_range(ex.track);
            (
                track.title.get(locale),
                app.range_progress(track_range.clone()),
                app.range_status(track_range),
            )
        } else {
            (mode.title(), app.mode_progress(mode), app.mode_status())
        };
        progress_bar(
            &mut MaxLenWriter::new(stdout, self.term_width as usize),
            label,
            done,
            total,
            self.term_width,
            status,
        )?;
        stdout.write_all(b"\n")?;

        for (j, journey) in course.journeys.iter().enumerate() {
            let (done, total, status) = app.journey_progress(j);
            if total == 0 || mode == Mode::Social {
                continue;
            }
            let title = journey.title.get(locale);
            let title = if title.is_empty() {
                journey.id.as_str()
            } else {
                title
            };
            stdout.queue(SetForegroundColor(status.color()))?;
            stdout.write_all(fill(t.journey_status, &[&title, &done, &total]).as_bytes())?;
            stdout.queue(ResetColor)?;
            stdout.write_all(b"\n")?;
        }
        if let Some(reminder) = review::reminder(app) {
            stdout.queue(SetForegroundColor(Color::DarkCyan))?;
            stdout.write_all(reminder.as_bytes())?;
            stdout.queue(ResetColor)?;
            stdout.write_all(b"\n")?;
        }

        stdout.write_all(b"\n")?;
        if ex.kind == Kind::Milestone {
            self.render_prerequisites(stdout)?;
        }
        write!(stdout, "{}: ", t.track)?;
        stdout.queue(SetForegroundColor(
            app.range_status(app.track_range(ex.track)).color(),
        ))?;
        stdout.write_all(track.title.get(locale).as_bytes())?;
        stdout.queue(ResetColor)?;
        write!(stdout, "  {}: ", t.chapter)?;
        stdout.queue(SetForegroundColor(
            app.range_status(app.chapter_range(ex.chapter)).color(),
        ))?;
        let chapter_title = chapter.title.get(locale);
        if chapter_title.is_empty() {
            stdout.write_all(chapter.id.as_bytes())?;
        } else {
            write!(stdout, "{chapter_title} ({})", chapter.id)?;
        }
        stdout.queue(ResetColor)?;
        stdout.write_all(b"\n")?;

        write!(stdout, "{}: ", t.exercise)?;
        stdout.queue(SetAttribute(Attribute::Bold))?;
        stdout.queue(SetForegroundColor(app.exercise_status(ind).color()))?;
        stdout.write_all(ex.name.as_bytes())?;
        stdout.queue(ResetColor)?;
        for (tag, color) in exercise_tags(app, ind) {
            stdout.write_all(b" ")?;
            stdout.queue(SetForegroundColor(color))?;
            stdout.write_all(tag.as_bytes())?;
            stdout.queue(ResetColor)?;
        }
        stdout.write_all(b"\n")?;

        if ex.kind != Kind::Question {
            let label = if ex.kind == Kind::File {
                t.file
            } else {
                t.folder
            };
            write!(stdout, "{label}: ")?;
            path_link(
                stdout,
                &exercise::display_path(course, ex),
                Color::Blue,
                links,
            )?;
            stdout.write_all(b"\n")?;
        }
        if let Some(readme) = exercise::task_readme(ex) {
            write!(stdout, "{}: ", t.task)?;
            path_link(stdout, &readme, Color::Blue, links)?;
            stdout.write_all(b"\n")?;
        }
        write!(stdout, "{}: ", t.theory)?;
        path_link(stdout, &chapter.readme(), Color::Blue, links)?;
        stdout.write_all(b"\n\n")
    }

    fn render_prerequisites(&self, stdout: &mut StdoutLock) -> io::Result<()> {
        let t = tr();
        let app = &self.app_state;
        let pre = mode::prerequisites(app, app.current_exercise_ind());
        if pre.all_done() {
            if app.mode() == Mode::Social {
                stdout
                    .queue(SetAttribute(Attribute::Bold))?
                    .queue(SetForegroundColor(Color::Green))?;
                stdout.write_all(t.prereq_all_done.as_bytes())?;
                stdout.queue(ResetColor)?;
                stdout.write_all(b"\n\n")?;
            }
            return Ok(());
        }
        let column = pre
            .tracks
            .iter()
            .map(|p| p.title.chars().count() + 2)
            .chain(pre.topics.iter().map(|p| p.title.chars().count() + 4))
            .max()
            .unwrap_or(0)
            .min(44);
        let row = |stdout: &mut StdoutLock, indent: &str, p: &Part| -> io::Result<()> {
            let width = column - indent.len();
            let title = p.title.chars().take(width).collect::<String>();
            write!(stdout, "{indent}{title:<width$}  ")?;
            stdout.queue(SetForegroundColor(p.color()))?;
            write!(stdout, "{} {:>3}%", mode::bar(p, 10), p.percent())?;
            stdout.queue(ResetColor)?;
            stdout.queue(SetForegroundColor(Color::DarkGrey))?;
            write!(stdout, "  {}/{}", p.done, p.total)?;
            stdout.queue(ResetColor)?;
            stdout.write_all(b"\n")
        };

        stdout
            .queue(SetAttribute(Attribute::Bold))?
            .queue(SetForegroundColor(Color::Yellow))?;
        stdout.write_all(fill(t.prereq_need, &[&pre.percent()]).as_bytes())?;
        stdout.queue(ResetColor)?;
        stdout.write_all(b"\n")?;
        for p in &pre.tracks {
            row(stdout, "  ", p)?;
        }
        if !pre.topics.is_empty() {
            writeln!(stdout, "  {}", t.prereq_topics)?;
            for p in &pre.topics {
                row(stdout, "    ", p)?;
            }
        }
        stdout.write_all(b"\n")
    }

    fn render_celebration(&self, stdout: &mut StdoutLock) -> io::Result<()> {
        let app = &self.app_state;
        let ex = app.current_exercise();
        let locale = app.locale();
        let width = usize::from(self.term_width).clamp(20, 72);
        let line = "=".repeat(width);
        stdout.queue(SetForegroundColor(Color::Magenta))?;
        writeln!(stdout, "{line}")?;
        stdout.queue(SetAttribute(Attribute::Bold))?;
        writeln!(stdout, "  {}", fill(tr().milestone_done, &[&ex.name]))?;
        stdout.queue(ResetColor)?;
        let story = ex.story.get(locale).trim();
        if !story.is_empty() {
            for l in story.lines() {
                writeln!(stdout, "  {l}")?;
            }
        }
        if let Some(j) = ex.journey {
            let (done, total, status) = app.journey_progress(j);
            let title = app.course().journeys[j].title.get(locale);
            stdout.write_all(b"  ")?;
            progress_bar(
                &mut MaxLenWriter::new(stdout, width),
                title,
                done,
                total,
                width as u16 - 2,
                status,
            )?;
            stdout.write_all(b"\n")?;
        }
        stdout.queue(SetForegroundColor(Color::Magenta))?;
        writeln!(stdout, "{line}")?;
        stdout.queue(ResetColor)?;
        Ok(())
    }

    pub fn render(&self, stdout: &mut StdoutLock) -> io::Result<()> {
        stdout.write_all(b"\n")?;
        clear_terminal(stdout)?;

        let t = tr();
        let locale = self.app_state.locale();
        let ex = self.app_state.current_exercise();

        if let Some(diff) = &self.diff {
            let mut buf = Vec::with_capacity(4096);
            diff::render_view(
                &mut buf,
                diff.files.as_deref(),
                diff.selected,
                ex.why.get(locale),
            );
            stdout.write_all(&buf)?;
            stdout.write_all(b"\n")?;
        } else if ex.kind == Kind::Question {
            self.render_question(stdout)?;
        } else {
            stdout.write_all(&self.output)?;
        }

        if self.show_hint && self.diff.is_none() {
            if ex.hints(locale).len() <= 1 {
                stdout
                    .queue(SetAttributes(HEADING_ATTRIBUTES))?
                    .queue(SetForegroundColor(Color::Cyan))?;
                stdout.write_all(t.hint.as_bytes())?;
                stdout.queue(ResetColor)?;
                stdout.write_all(b"\n")?;
            }
            let opened = self
                .app_state
                .hints_opened(self.app_state.current_exercise_ind());
            write_hints(stdout, ex, locale, opened)?;
            stdout.write_all(b"\n")?;
        }

        if self.done() && self.diff.is_none() {
            if ex.kind == Kind::Milestone {
                self.render_celebration(stdout)?;
            } else {
                stdout
                    .queue(SetAttribute(Attribute::Bold))?
                    .queue(SetForegroundColor(Color::Green))?;
                stdout.write_all(t.exercise_done.as_bytes())?;
                stdout.queue(ResetColor)?;
                stdout.write_all(b"\n")?;
            }

            if let DoneStatus::DoneWithSolution(solution_path) = &self.done_status {
                solution_link_line(stdout, solution_path, self.app_state.emit_file_links())?;
            }

            stdout.write_all(t.when_done.as_bytes())?;
            stdout.write_all(b"\n\n")?;
        }

        if let Some(notice) = &self.notice {
            stdout.queue(SetForegroundColor(Color::Yellow))?;
            stdout.write_all(notice.as_bytes())?;
            stdout.queue(ResetColor)?;
            stdout.write_all(b"\n\n")?;
        }

        if !self.visual_menu.is_empty() {
            stdout.queue(SetAttribute(Attribute::Bold))?;
            stdout.write_all(t.visuals_menu.as_bytes())?;
            stdout.queue(ResetColor)?;
            stdout.write_all(b"\n")?;
            for (i, (title, _)) in self.visual_menu.iter().enumerate().take(9) {
                writeln!(stdout, "  {}) {title}", i + 1)?;
            }
            stdout.write_all(b"\n")?;
        }

        self.render_location(stdout)?;
        self.show_prompt(stdout)
    }

    pub fn show_hint(&mut self, stdout: &mut StdoutLock) -> Result<()> {
        let ind = self.app_state.current_exercise_ind();
        let ex = self.app_state.exercise(ind);
        if ex.exam {
            self.notice = Some(tr().exam_no_hints.to_owned());
        } else if !self.show_hint && self.app_state.hints_opened(ind) > 0 {
            self.show_hint = true;
        } else {
            self.app_state.open_next_hint(ind)?;
            self.show_hint = true;
        }
        self.diff = None;
        self.render(stdout)?;
        Ok(())
    }

    pub fn update_term_width(&mut self, width: u16, stdout: &mut StdoutLock) -> io::Result<()> {
        if self.term_width != width {
            self.term_width = width;
            self.render(stdout)?;
        }
        Ok(())
    }
}
