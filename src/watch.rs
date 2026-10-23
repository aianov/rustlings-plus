use anyhow::{Error, Result};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    io::{self, Write},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
        mpsc::channel,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    app_state::{AppState, ExercisesProgress},
    i18n::tr,
    list, mode,
    watch::{
        notify_event::{NotifyEventHandler, WatchMap},
        state::{Confirm, WatchState},
        terminal_event::InputEvent,
    },
};

mod notify_event;
mod state;
mod terminal_event;

pub use state::{exercise_tags, write_hints};

static EXERCISE_RUNNING: AtomicBool = AtomicBool::new(false);
static QUIET_UNTIL_MS: AtomicU64 = AtomicU64::new(0);
const QUIET_AFTER_RUN_MS: u64 = 700;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

pub fn events_paused() -> bool {
    EXERCISE_RUNNING.load(Relaxed) || now_ms() < QUIET_UNTIL_MS.load(Relaxed)
}

#[must_use = "When the guard is dropped, the input is unpaused"]
pub struct InputPauseGuard(());

impl InputPauseGuard {
    pub fn scoped_pause() -> Self {
        EXERCISE_RUNNING.store(true, Relaxed);
        Self(())
    }
}

impl Drop for InputPauseGuard {
    fn drop(&mut self) {
        QUIET_UNTIL_MS.store(now_ms() + QUIET_AFTER_RUN_MS, Relaxed);
        EXERCISE_RUNNING.store(false, Relaxed);
    }
}

enum WatchEvent {
    Input(InputEvent),
    FileChange { target: usize },
    TerminalResize { width: u16 },
    NotifyErr(notify::Error),
    TerminalEventErr(io::Error),
}

#[must_use]
enum WatchExit {
    Shutdown,
    List,
    Mode,
}

fn watched_dirs(app_state: &AppState) -> Vec<String> {
    let root = &app_state.ctx().root;
    let mut dirs = vec!["exercises".to_owned()];
    for journey in &app_state.course().journeys {
        if root.join(&journey.dir).is_dir() {
            dirs.push(journey.dir.clone());
        }
    }
    dirs
}

fn run_watch(app_state: &mut AppState, manual_run: bool) -> Result<WatchExit> {
    let (watch_event_sender, watch_event_receiver) = channel();
    let mut dirs = watched_dirs(app_state);

    let mut watcher_guard = if manual_run {
        None
    } else {
        let map = WatchMap::new(app_state.course(), &app_state.ctx().root);
        let handler = NotifyEventHandler::build(watch_event_sender.clone(), map)?;
        let mut watcher = RecommendedWatcher::new(
            handler,
            Config::default()
                .with_follow_symlinks(false)
                .with_poll_interval(Duration::from_secs(1)),
        )
        .inspect_err(|_| eprintln!("{}", tr().notify_err))?;
        for dir in &dirs {
            watcher
                .watch(&app_state.ctx().root.join(dir), RecursiveMode::Recursive)
                .inspect_err(|_| eprintln!("{}", tr().notify_err))?;
        }
        Some(watcher)
    };

    let mut watch_state = WatchState::build(app_state, watch_event_sender, manual_run)?;
    let mut stdout = io::stdout().lock();

    watch_state.run_current_exercise(&mut stdout)?;

    while let Ok(event) = watch_event_receiver.recv() {
        let keeps_menu = matches!(
            event,
            WatchEvent::Input(InputEvent::Choose(_) | InputEvent::Esc)
        );
        if !keeps_menu && matches!(event, WatchEvent::Input(_)) && watch_state.menu_open() {
            watch_state.close_menu(&mut stdout)?;
        }
        match event {
            WatchEvent::Input(InputEvent::Next) => match watch_state.next_exercise(&mut stdout)? {
                ExercisesProgress::AllDone => break,
                ExercisesProgress::NewPending => watch_state.run_current_exercise(&mut stdout)?,
                ExercisesProgress::CurrentPending => watch_state.render(&mut stdout)?,
            },
            WatchEvent::Input(InputEvent::Run) => watch_state.run_current_exercise(&mut stdout)?,
            WatchEvent::Input(InputEvent::Hint) => watch_state.show_hint(&mut stdout)?,
            WatchEvent::Input(InputEvent::List) => return Ok(WatchExit::List),
            WatchEvent::Input(InputEvent::Mode) => return Ok(WatchExit::Mode),
            WatchEvent::Input(InputEvent::Reset) => {
                watch_state.confirm(&mut stdout, Confirm::Reset)?;
            }
            WatchEvent::Input(InputEvent::Baseline) => {
                watch_state.confirm(&mut stdout, Confirm::Baseline)?;
            }
            WatchEvent::Input(InputEvent::Lang) => watch_state.switch_language(&mut stdout)?,
            WatchEvent::Input(InputEvent::Visuals) => watch_state.visuals(&mut stdout)?,
            WatchEvent::Input(InputEvent::Choose(ind)) => watch_state.choose(ind, &mut stdout)?,
            WatchEvent::Input(InputEvent::Diff) => watch_state.toggle_diff(&mut stdout)?,
            WatchEvent::Input(InputEvent::Answer) => watch_state.start_answer(&mut stdout)?,
            WatchEvent::Input(InputEvent::Enter) => watch_state.enter(&mut stdout)?,
            WatchEvent::Input(InputEvent::Esc) => watch_state.escape(&mut stdout)?,
            WatchEvent::Input(InputEvent::Char(c)) => watch_state.type_char(c, &mut stdout)?,
            WatchEvent::Input(InputEvent::Backspace) => watch_state.backspace(&mut stdout)?,
            WatchEvent::Input(InputEvent::Other) => (),
            WatchEvent::Input(InputEvent::Quit) => {
                stdout.write_all(b"q\n")?;
                stdout.write_all(tr().quit_msg.as_bytes())?;
                break;
            }
            WatchEvent::FileChange { target } => {
                watch_state.handle_file_change(target, &mut stdout)?;
            }
            WatchEvent::TerminalResize { width } => {
                watch_state.update_term_width(width, &mut stdout)?;
            }
            WatchEvent::NotifyErr(e) => return Err(Error::from(e).context(tr().notify_err)),
            WatchEvent::TerminalEventErr(e) => {
                return Err(Error::from(e).context("Terminal event listener failed"));
            }
        }
        if let Some(watcher) = watcher_guard.as_mut() {
            for dir in watched_dirs(watch_state.app_state()) {
                if !dirs.contains(&dir) {
                    watcher
                        .watch(
                            &watch_state.app_state().ctx().root.join(&dir),
                            RecursiveMode::Recursive,
                        )
                        .inspect_err(|_| eprintln!("{}", tr().notify_err))?;
                    dirs.push(dir);
                }
            }
        }
    }

    Ok(WatchExit::Shutdown)
}

fn watch_list_loop(app_state: &mut AppState, manual_run: bool) -> Result<()> {
    loop {
        match run_watch(app_state, manual_run)? {
            WatchExit::Shutdown => break Ok(()),
            WatchExit::List => list::list(app_state)?,
            WatchExit::Mode => {
                mode::choose(app_state, false)?;
            }
        }
    }
}

pub fn watch(app_state: &mut AppState, manual_run: bool) -> Result<()> {
    #[cfg(not(windows))]
    {
        let stdin_fd = rustix::stdio::stdin();
        let mut termios = rustix::termios::tcgetattr(stdin_fd)?;
        let original_local_modes = termios.local_modes;
        termios.local_modes -=
            rustix::termios::LocalModes::ICANON | rustix::termios::LocalModes::ECHO;
        rustix::termios::tcsetattr(stdin_fd, rustix::termios::OptionalActions::Now, &termios)?;

        let res = watch_list_loop(app_state, manual_run);

        termios.local_modes = original_local_modes;
        rustix::termios::tcsetattr(stdin_fd, rustix::termios::OptionalActions::Now, &termios)?;

        res
    }

    #[cfg(windows)]
    watch_list_loop(app_state, manual_run)
}
