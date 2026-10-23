use anyhow::{Context, Result};
use crossterm::{
    QueueableCommand,
    cursor::{MoveTo, MoveToNextLine},
    style::{
        Attribute, Attributes, Color, ResetColor, SetAttribute, SetAttributes, SetForegroundColor,
    },
    terminal::{self, BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate},
};
use std::io::{self, StdoutLock, Write};

use crate::{
    app_state::{AppState, Status},
    exercise,
    i18n::{fill, tr},
    list::scroll_state::ScrollState,
    mode::Mode,
    term::{CountedWrite, MaxLenWriter, progress_bar},
    visuals,
    watch::exercise_tags,
};

const SELECTED_ROW_ATTRIBUTES: Attributes = Attributes::none()
    .with(Attribute::Reverse)
    .with(Attribute::Bold);
const FOOTER_HEIGHT: u16 = 4;

fn next_ln(stdout: &mut StdoutLock) -> io::Result<()> {
    stdout
        .queue(Clear(ClearType::UntilNewLine))?
        .queue(MoveToNextLine(1))?;
    Ok(())
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Filter {
    Done,
    Pending,
    None,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Row {
    Track(usize),
    Chapter(usize),
    Exercise(usize),
}

pub struct ListState<'a> {
    pub message: String,
    pub search_query: String,
    pub pending_reset: Option<usize>,
    pub visual_menu: Vec<(String, String)>,
    app_state: &'a mut AppState,
    scroll_state: ScrollState,
    rows: Vec<Row>,
    expanded: Vec<bool>,
    collapsed_tracks: Vec<bool>,
    track_filter: Option<usize>,
    filter: Filter,
    term_width: u16,
    term_height: u16,
    show_footer: bool,
}

impl<'a> ListState<'a> {
    pub fn build(app_state: &'a mut AppState, stdout: &mut StdoutLock) -> Result<Self> {
        stdout.queue(Clear(ClearType::All))?;
        let course = app_state.course();
        let current = app_state.current_exercise_ind();
        let mut expanded = vec![false; course.chapters.len()];
        expanded[course.exercises[current].chapter] = true;

        let (width, height) = terminal::size().context("Failed to get the terminal size")?;
        let mut slf = Self {
            message: String::with_capacity(128),
            search_query: String::new(),
            pending_reset: None,
            visual_menu: Vec::new(),
            collapsed_tracks: vec![false; course.tracks.len()],
            app_state,
            scroll_state: ScrollState::new(0, None, 5),
            rows: Vec::new(),
            expanded,
            track_filter: None,
            filter: Filter::None,
            term_width: 0,
            term_height: 0,
            show_footer: true,
        };
        slf.rebuild(Some(Row::Exercise(current)));
        slf.set_term_size(width, height);
        slf.draw(stdout)?;
        Ok(slf)
    }

    fn exercise_visible(&self, ind: usize) -> bool {
        let done = self.app_state.is_done(ind);
        let by_filter = match self.filter {
            Filter::Done => done,
            Filter::Pending => !done,
            Filter::None => true,
        };
        by_filter
            && self.app_state.in_mode(ind)
            && (self.search_query.is_empty()
                || self
                    .app_state
                    .exercise(ind)
                    .name
                    .contains(&self.search_query))
    }

    pub fn rebuild(&mut self, keep: Option<Row>) {
        let course = self.app_state.course();
        let moded = self.app_state.mode() != Mode::Story;
        let narrowing = moded || self.filter != Filter::None || !self.search_query.is_empty();
        let searching = !self.search_query.is_empty();
        let mut rows = Vec::with_capacity(256);
        for (t, track) in course.tracks.iter().enumerate() {
            if self.track_filter.is_some_and(|f| f != t) {
                continue;
            }
            let track_row = rows.len();
            rows.push(Row::Track(t));
            if self.collapsed_tracks[t] && !searching {
                continue;
            }
            for c in track.chapters.clone() {
                let visible = course.chapters[c]
                    .exercises
                    .clone()
                    .filter(|&e| self.exercise_visible(e))
                    .collect::<Vec<_>>();
                if narrowing && visible.is_empty() {
                    continue;
                }
                rows.push(Row::Chapter(c));
                if self.expanded[c] || searching {
                    rows.extend(visible.into_iter().map(Row::Exercise));
                }
            }
            if (searching || moded) && rows.len() == track_row + 1 {
                rows.pop();
            }
        }
        self.rows = rows;
        self.scroll_state.set_n_rows(self.rows.len());
        if let Some(keep) = keep {
            if let Some(pos) = self.rows.iter().position(|r| *r == keep) {
                self.scroll_state.set_selected(pos);
            } else if let Some(pos) = self.parent_position(keep) {
                self.scroll_state.set_selected(pos);
            }
        }
    }

    fn parent_position(&self, row: Row) -> Option<usize> {
        let course = self.app_state.course();
        let (chapter, track) = match row {
            Row::Exercise(e) => (Some(course.exercises[e].chapter), course.exercises[e].track),
            Row::Chapter(c) => (None, course.chapters[c].track),
            Row::Track(t) => (None, t),
        };
        chapter
            .and_then(|c| self.rows.iter().position(|r| *r == Row::Chapter(c)))
            .or_else(|| self.rows.iter().position(|r| *r == Row::Track(track)))
    }

    pub fn set_term_size(&mut self, width: u16, height: u16) {
        self.term_width = width;
        self.term_height = height;
        if height == 0 {
            return;
        }
        let header_height = 1;
        self.show_footer = height > header_height + FOOTER_HEIGHT;
        self.scroll_state.set_max_n_rows_to_display(
            height.saturating_sub(header_height + u16::from(self.show_footer) * FOOTER_HEIGHT)
                as usize,
        );
    }

    pub fn selected_row(&self) -> Option<Row> {
        self.scroll_state
            .selected()
            .and_then(|i| self.rows.get(i).copied())
    }

    fn write_status_text(writer: &mut MaxLenWriter, status: Status, text: &str) -> io::Result<()> {
        writer.stdout.queue(SetForegroundColor(status.color()))?;
        writer.write_str(text)?;
        writer.stdout.queue(SetForegroundColor(Color::Reset))?;
        Ok(())
    }

    fn draw_name(&self, writer: &mut MaxLenWriter, name: &str, status: Status) -> io::Result<()> {
        writer.stdout.queue(SetForegroundColor(status.color()))?;
        if !self.search_query.is_empty()
            && let Some(ind) = name.find(&self.search_query)
        {
            let (pre, rest) = name.split_at(ind);
            let (hit, post) = rest.split_at(self.search_query.len());
            writer.write_str(pre)?;
            writer.stdout.queue(SetForegroundColor(Color::Magenta))?;
            writer.write_str(hit)?;
            writer.stdout.queue(SetForegroundColor(status.color()))?;
            writer.write_str(post)?;
        } else {
            writer.write_str(name)?;
        }
        writer.stdout.queue(SetForegroundColor(Color::Reset))?;
        Ok(())
    }

    fn draw_row(&self, writer: &mut MaxLenWriter, row: Row) -> io::Result<()> {
        let app = &*self.app_state;
        let course = app.course();
        let locale = app.locale();
        match row {
            Row::Track(t) => {
                let track = &course.tracks[t];
                let range = app.track_range(t);
                let (done, total) = app.range_progress(range.clone());
                let status = app.range_status(range);
                let fold = if self.collapsed_tracks[t] {
                    "▸ "
                } else {
                    "▾ "
                };
                writer.write_str(fold)?;
                writer.stdout.queue(SetAttribute(Attribute::Bold))?;
                Self::write_status_text(writer, status, track.title.get(locale))?;
                writer
                    .stdout
                    .queue(SetAttribute(Attribute::NormalIntensity))?;
                if track.chapters.is_empty() {
                    writer.write_str(&format!("  ({}, {})", track.id, tr().coming_soon))?;
                } else {
                    writer.write_str(&format!("  {done}/{total}  ({})", track.id))?;
                }
            }
            Row::Chapter(c) => {
                let chapter = &course.chapters[c];
                let range = app.chapter_range(c);
                let (done, total) = app.range_progress(range.clone());
                let status = app.range_status(range);
                let fold = if self.expanded[c] { "▾ " } else { "▸ " };
                writer.write_ascii(b"   ")?;
                writer.write_str(fold)?;
                let title = chapter.title.get(locale);
                let label = if title.is_empty() {
                    chapter.id.clone()
                } else {
                    format!("{} {title}", chapter.id)
                };
                Self::write_status_text(writer, status, &label)?;
                writer.write_str(&format!("  {done}/{total}"))?;
                if !chapter.visuals.is_empty() {
                    writer.stdout.queue(SetForegroundColor(Color::DarkCyan))?;
                    writer.write_str(&format!("  [{}]", tr().key_visuals))?;
                    writer.stdout.queue(SetForegroundColor(Color::Reset))?;
                }
            }
            Row::Exercise(e) => {
                let status = app.exercise_status(e);
                writer.write_ascii(b"       ")?;
                if e == app.current_exercise_ind() {
                    writer.stdout.queue(SetForegroundColor(Color::Red))?;
                    writer.write_ascii(b">")?;
                    writer.stdout.queue(SetForegroundColor(Color::Reset))?;
                } else {
                    writer.write_ascii(b" ")?;
                }
                if app.is_done(e) {
                    Self::write_status_text(writer, Status::Done, "[x] ")?;
                } else {
                    writer.write_ascii(b"[ ] ")?;
                }
                self.draw_name(writer, &course.exercises[e].name, status)?;
                for (tag, color) in exercise_tags(app, e) {
                    writer.write_ascii(b" ")?;
                    writer.stdout.queue(SetForegroundColor(color))?;
                    writer.write_str(&tag)?;
                    writer.stdout.queue(SetForegroundColor(Color::Reset))?;
                }
            }
        }
        Ok(())
    }

    fn draw_rows(&self, stdout: &mut StdoutLock) -> io::Result<usize> {
        let offset = self.scroll_state.offset();
        let mut n = 0;
        for (i, row) in self
            .rows
            .iter()
            .enumerate()
            .skip(offset)
            .take(self.scroll_state.max_n_rows_to_display())
        {
            let mut writer = MaxLenWriter::new(stdout, self.term_width as usize);
            if self.scroll_state.selected() == Some(i) {
                writer.add_to_len(2);
                writer.stdout.write_all("🦀".as_bytes())?;
                writer
                    .stdout
                    .queue(SetAttributes(SELECTED_ROW_ATTRIBUTES))?;
            } else {
                writer.write_ascii(b"  ")?;
            }
            self.draw_row(&mut writer, *row)?;
            next_ln(stdout)?;
            stdout.queue(ResetColor)?;
            n += 1;
        }
        Ok(n)
    }

    fn hotkey(writer: &mut MaxLenWriter, key: &str, label: &str) -> io::Result<()> {
        writer
            .stdout
            .queue(SetForegroundColor(Color::Yellow))?
            .queue(SetAttribute(Attribute::Bold))?;
        writer.write_str(key)?;
        writer.stdout.queue(ResetColor)?;
        writer.write_str(": ")?;
        writer.write_str(label)?;
        writer.write_str(" | ")
    }

    fn draw_footer(&self, stdout: &mut StdoutLock) -> io::Result<()> {
        let app = &*self.app_state;
        let course = app.course();
        let locale = app.locale();
        let t = tr();
        let width = self.term_width as usize;
        let selected = self.selected_row();

        let track = match selected {
            Some(Row::Track(t)) => t,
            Some(Row::Chapter(c)) => course.chapters[c].track,
            Some(Row::Exercise(e)) => course.exercises[e].track,
            None => app.current_exercise().track,
        };
        let range = app.track_range(track);
        let (done, total) = app.range_progress(range.clone());
        progress_bar(
            &mut MaxLenWriter::new(stdout, width),
            course.tracks[track].title.get(locale),
            done,
            total,
            self.term_width,
            app.range_status(range),
        )?;
        next_ln(stdout)?;

        let mut writer = MaxLenWriter::new(stdout, width);
        match selected {
            Some(Row::Exercise(e)) => {
                let ex = &course.exercises[e];
                let path = exercise::display_path(course, ex);
                writer.stdout.queue(SetForegroundColor(Color::Blue))?;
                writer.write_str(&path)?;
                writer.stdout.queue(ResetColor)?;
                let state = if app.is_done(e) {
                    t.done_word
                } else {
                    t.pending_word
                };
                writer.write_str(&format!("  ({state})"))?;
            }
            Some(Row::Chapter(c)) => {
                writer.stdout.queue(SetForegroundColor(Color::Blue))?;
                writer.write_str(&course.chapters[c].readme())?;
                writer.stdout.queue(ResetColor)?;
            }
            Some(Row::Track(t)) => {
                writer.write_str(course.tracks[t].description.get(locale))?;
            }
            None => (),
        }
        next_ln(stdout)?;

        if !self.message.is_empty() || !self.visual_menu.is_empty() {
            let mut writer = MaxLenWriter::new(stdout, width);
            writer.stdout.queue(SetForegroundColor(Color::Magenta))?;
            if self.visual_menu.is_empty() {
                writer.write_str(&self.message)?;
            } else {
                writer.write_str(t.visuals_menu)?;
                for (i, (title, _)) in self.visual_menu.iter().enumerate().take(9) {
                    writer.write_str(&format!("  {}) {title}", i + 1))?;
                }
            }
            stdout.queue(ResetColor)?;
            next_ln(stdout)?;
            next_ln(stdout)?;
            return Ok(());
        }

        let mut writer = MaxLenWriter::new(stdout, width);
        Self::hotkey(&mut writer, "↑↓/jk", t.list_hint_move)?;
        Self::hotkey(&mut writer, "↩/c", t.list_hint_open)?;
        Self::hotkey(&mut writer, "␣/←→", t.list_hint_fold)?;
        let track_label = match self.track_filter {
            Some(f) => course.tracks[f].id.clone(),
            None => t.track_all.to_owned(),
        };
        Self::hotkey(
            &mut writer,
            "Tab",
            &format!("{} ({track_label})", t.list_hint_tracks),
        )?;
        next_ln(stdout)?;

        let mut writer = MaxLenWriter::new(stdout, width);
        Self::hotkey(&mut writer, "s", t.list_hint_search)?;
        Self::hotkey(&mut writer, "d/p", t.list_hint_filter)?;
        Self::hotkey(&mut writer, "r", t.list_hint_reset)?;
        Self::hotkey(&mut writer, "v", t.list_hint_visuals)?;
        writer
            .stdout
            .queue(SetForegroundColor(Color::Yellow))?
            .queue(SetAttribute(Attribute::Bold))?;
        writer.write_str("q")?;
        writer.stdout.queue(ResetColor)?;
        writer.write_str(": ")?;
        writer.write_str(t.list_hint_quit)?;
        next_ln(stdout)
    }

    pub fn draw(&mut self, stdout: &mut StdoutLock) -> io::Result<()> {
        if self.term_height == 0 {
            return Ok(());
        }
        stdout.queue(BeginSynchronizedUpdate)?.queue(MoveTo(0, 0))?;

        let mut writer = MaxLenWriter::new(stdout, self.term_width as usize);
        let (done, total) = (self.app_state.n_done(), self.app_state.len());
        writer.stdout.queue(SetAttribute(Attribute::Bold))?;
        writer.write_str(&format!("Rustlings  {done}/{total}"))?;
        writer.stdout.queue(ResetColor)?;
        let course = self.app_state.course();
        for (j, journey) in course.journeys.iter().enumerate() {
            let (done, total, status) = self.app_state.journey_progress(j);
            if total == 0 {
                continue;
            }
            let title = journey.title.get(self.app_state.locale());
            writer.write_str("  |  ")?;
            writer.stdout.queue(SetForegroundColor(status.color()))?;
            writer.write_str(&fill(tr().journey_status, &[&title, &done, &total]))?;
            writer.stdout.queue(ResetColor)?;
        }
        if !self.search_query.is_empty() {
            writer.write_str("  |  ")?;
            writer.write_str(tr().search)?;
            writer.write_str(&self.search_query)?;
        }
        next_ln(stdout)?;

        let n = self.draw_rows(stdout)?;
        for _ in n..self.scroll_state.max_n_rows_to_display() {
            next_ln(stdout)?;
        }
        if self.show_footer {
            self.draw_footer(stdout)?;
        }
        stdout.queue(EndSynchronizedUpdate)?.flush()
    }

    pub fn filter(&self) -> Filter {
        self.filter
    }

    pub fn set_filter(&mut self, filter: Filter) {
        self.filter = filter;
        self.rebuild(self.selected_row());
    }

    pub fn cycle_track(&mut self, forward: bool) {
        let n = self.app_state.course().tracks.len();
        self.track_filter = match (self.track_filter, forward) {
            (None, true) => Some(0),
            (None, false) => n.checked_sub(1),
            (Some(t), true) if t + 1 < n => Some(t + 1),
            (Some(t), false) if t > 0 => Some(t - 1),
            _ => None,
        };
        let keep = self.track_filter.map(Row::Track).or(self.selected_row());
        self.rebuild(keep);
        if let Some(t) = self.track_filter
            && let Some(pos) = self.rows.iter().position(|r| *r == Row::Track(t))
        {
            self.scroll_state.set_selected(pos);
        }
    }

    pub fn select_next(&mut self) {
        self.scroll_state.select_next();
    }

    pub fn select_previous(&mut self) {
        self.scroll_state.select_previous();
    }

    pub fn select_first(&mut self) {
        self.scroll_state.select_first();
    }

    pub fn select_last(&mut self) {
        self.scroll_state.select_last();
    }

    pub fn page(&mut self, down: bool) {
        let step = self.scroll_state.max_n_rows_to_display().max(1);
        for _ in 0..step {
            if down {
                self.scroll_state.select_next();
            } else {
                self.scroll_state.select_previous();
            }
        }
    }

    pub fn toggle(&mut self, expand: Option<bool>) {
        let Some(row) = self.selected_row() else {
            return;
        };
        match row {
            Row::Track(t) => {
                let collapsed = &mut self.collapsed_tracks[t];
                *collapsed = expand.map_or(!*collapsed, |e| !e);
                self.rebuild(Some(row));
            }
            Row::Chapter(c) => {
                let expanded = &mut self.expanded[c];
                *expanded = expand.unwrap_or(!*expanded);
                self.rebuild(Some(row));
            }
            Row::Exercise(e) => {
                if expand == Some(false) || expand.is_none() {
                    let chapter = self.app_state.exercise(e).chapter;
                    self.expanded[chapter] = false;
                    self.rebuild(Some(Row::Chapter(chapter)));
                }
            }
        }
    }

    pub fn apply_search_query(&mut self) {
        let keep = self.selected_row();
        self.rebuild(keep);
        if self.search_query.is_empty() {
            return;
        }
        match self.rows.iter().position(|r| matches!(r, Row::Exercise(_))) {
            Some(pos) => self.scroll_state.set_selected(pos),
            None => self.message.push_str(tr().not_found),
        }
    }

    pub fn ask_reset(&mut self) {
        match self.selected_row() {
            Some(Row::Exercise(e)) => {
                self.pending_reset = Some(e);
                self.message = fill(tr().reset_ask, &[&self.app_state.exercise(e).name]);
            }
            _ => self.message.push_str(tr().nothing_selected),
        }
    }

    pub fn confirm_reset(&mut self) -> Result<()> {
        let Some(e) = self.pending_reset.take() else {
            return Ok(());
        };
        let note = self.app_state.reset_exercise(e)?;
        let path = exercise::display_path(self.app_state.course(), self.app_state.exercise(e));
        self.message = note.unwrap_or_else(|| fill(tr().reset_done, &[&path]));
        Ok(())
    }

    pub fn open_visuals(&mut self) {
        let course = self.app_state.course();
        let chapter = match self.selected_row() {
            Some(Row::Chapter(c)) => c,
            Some(Row::Exercise(e)) => course.exercises[e].chapter,
            _ => self.app_state.current_exercise().chapter,
        };
        let list = visuals::chapter_visuals(course, chapter, self.app_state.locale());
        match list.len() {
            0 => self.message.push_str(tr().visuals_none),
            1 => self.open_visual(&list[0].1),
            _ => self.visual_menu = list,
        }
    }

    pub fn open_visual(&mut self, path: &str) {
        let ctx = self.app_state.ctx();
        self.message = match visuals::open(ctx.course, &ctx.root, path, ctx.locale) {
            Ok(message) => message,
            Err(e) => format!("{e:#}"),
        };
    }

    pub fn choose_visual(&mut self, ind: usize) {
        let chosen = self.visual_menu.get(ind).cloned();
        self.visual_menu.clear();
        if let Some((_, path)) = chosen {
            self.open_visual(&path);
        }
    }

    pub fn activate(&mut self) -> Result<bool> {
        match self.selected_row() {
            Some(Row::Exercise(e)) => {
                self.app_state.set_current_exercise_ind(e)?;
                Ok(true)
            }
            Some(_) => {
                self.toggle(None);
                Ok(false)
            }
            None => {
                self.message.push_str(tr().nothing_selected);
                Ok(false)
            }
        }
    }
}
