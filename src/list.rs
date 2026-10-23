use anyhow::{Context, Result};
use crossterm::{
    QueueableCommand, cursor,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseEventKind,
    },
    terminal::{
        DisableLineWrap, EnableLineWrap, EnterAlternateScreen, LeaveAlternateScreen,
        disable_raw_mode, enable_raw_mode,
    },
};
use std::io::{self, StdoutLock, Write};

use crate::{
    app_state::AppState,
    i18n::tr,
    list::state::{Filter, ListState},
};

mod scroll_state;
mod state;

fn handle_list(app_state: &mut AppState, stdout: &mut StdoutLock) -> Result<()> {
    let mut list_state = ListState::build(app_state, stdout)?;
    let mut is_searching = false;

    loop {
        match event::read().context("Failed to read terminal event")? {
            Event::Key(key) => {
                match key.kind {
                    KeyEventKind::Release => continue,
                    KeyEventKind::Press | KeyEventKind::Repeat => (),
                }

                list_state.message.clear();

                if list_state.pending_reset.is_some() {
                    if matches!(key.code, KeyCode::Char('y' | 'Y')) {
                        list_state.confirm_reset()?;
                    } else {
                        list_state.pending_reset = None;
                    }
                    list_state.draw(stdout)?;
                    continue;
                }

                if !list_state.visual_menu.is_empty() {
                    if let KeyCode::Char(c @ '1'..='9') = key.code {
                        list_state.choose_visual(c as usize - '1' as usize);
                    } else {
                        list_state.visual_menu.clear();
                    }
                    list_state.draw(stdout)?;
                    continue;
                }

                if is_searching {
                    match key.code {
                        KeyCode::Esc => {
                            is_searching = false;
                            list_state.search_query.clear();
                            list_state.apply_search_query();
                        }
                        KeyCode::Enter => is_searching = false,
                        KeyCode::Char(c) => {
                            list_state.search_query.push(c);
                            list_state.apply_search_query();
                        }
                        KeyCode::Backspace => {
                            list_state.search_query.pop();
                            list_state.apply_search_query();
                        }
                        KeyCode::Down => list_state.select_next(),
                        KeyCode::Up => list_state.select_previous(),
                        _ => continue,
                    }
                    list_state.draw(stdout)?;
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Esc => {
                        if list_state.search_query.is_empty() {
                            return Ok(());
                        }
                        list_state.search_query.clear();
                        list_state.apply_search_query();
                    }
                    KeyCode::Down | KeyCode::Char('j') => list_state.select_next(),
                    KeyCode::Up | KeyCode::Char('k') => list_state.select_previous(),
                    KeyCode::PageDown => list_state.page(true),
                    KeyCode::PageUp => list_state.page(false),
                    KeyCode::Home | KeyCode::Char('g') => list_state.select_first(),
                    KeyCode::End | KeyCode::Char('G') => list_state.select_last(),
                    KeyCode::Right => list_state.toggle(Some(true)),
                    KeyCode::Left => list_state.toggle(Some(false)),
                    KeyCode::Char(' ') => list_state.toggle(None),
                    KeyCode::Tab => list_state.cycle_track(true),
                    KeyCode::BackTab => list_state.cycle_track(false),
                    KeyCode::Char('d') => {
                        if list_state.filter() == Filter::Done {
                            list_state.set_filter(Filter::None);
                            list_state.message.push_str(tr().filter_off);
                        } else {
                            list_state.set_filter(Filter::Done);
                            list_state.message.push_str(tr().filter_done_on);
                        }
                    }
                    KeyCode::Char('p') => {
                        if list_state.filter() == Filter::Pending {
                            list_state.set_filter(Filter::None);
                            list_state.message.push_str(tr().filter_off);
                        } else {
                            list_state.set_filter(Filter::Pending);
                            list_state.message.push_str(tr().filter_pending_on);
                        }
                    }
                    KeyCode::Char('r') => list_state.ask_reset(),
                    KeyCode::Char('v') => list_state.open_visuals(),
                    KeyCode::Char('c') | KeyCode::Enter => {
                        if list_state.activate()? {
                            return Ok(());
                        }
                    }
                    KeyCode::Char('s' | '/') => {
                        is_searching = true;
                        list_state.apply_search_query();
                    }
                    _ => continue,
                }
            }
            Event::Mouse(event) => match event.kind {
                MouseEventKind::ScrollDown => list_state.select_next(),
                MouseEventKind::ScrollUp => list_state.select_previous(),
                _ => continue,
            },
            Event::Resize(width, height) => list_state.set_term_size(width, height),
            Event::FocusGained | Event::FocusLost => continue,
        }

        list_state.draw(stdout)?;
    }
}

pub fn list(app_state: &mut AppState) -> Result<()> {
    let mut stdout = io::stdout().lock();
    stdout
        .queue(EnterAlternateScreen)?
        .queue(cursor::Hide)?
        .queue(DisableLineWrap)?
        .queue(EnableMouseCapture)?;
    enable_raw_mode()?;

    let res = handle_list(app_state, &mut stdout);

    stdout
        .queue(LeaveAlternateScreen)?
        .queue(cursor::Show)?
        .queue(EnableLineWrap)?
        .queue(DisableMouseCapture)?
        .flush()?;
    disable_raw_mode()?;

    res
}
