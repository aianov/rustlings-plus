use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::sync::{
    atomic::{AtomicBool, Ordering::Relaxed},
    mpsc::{Receiver, Sender},
};

use crate::watch::{EXERCISE_RUNNING, WatchEvent};

pub static TEXT_INPUT: AtomicBool = AtomicBool::new(false);

pub enum InputEvent {
    Next,
    Run,
    Hint,
    List,
    Mode,
    Reset,
    Baseline,
    Lang,
    Visuals,
    Diff,
    Answer,
    Choose(usize),
    Enter,
    Esc,
    Char(char),
    Backspace,
    Other,
    Quit,
}

pub fn terminal_event_handler(
    sender: Sender<WatchEvent>,
    unpause_receiver: Receiver<()>,
    manual_run: bool,
) {
    let last_watch_event = loop {
        match event::read() {
            Ok(Event::Key(key)) => {
                match key.kind {
                    KeyEventKind::Release | KeyEventKind::Repeat => continue,
                    KeyEventKind::Press => (),
                }

                if EXERCISE_RUNNING.load(Relaxed) {
                    continue;
                }

                if TEXT_INPUT.load(Relaxed) {
                    let input_event = match key.code {
                        KeyCode::Char(c) => InputEvent::Char(c),
                        KeyCode::Backspace => InputEvent::Backspace,
                        KeyCode::Enter => InputEvent::Enter,
                        KeyCode::Esc => InputEvent::Esc,
                        _ => continue,
                    };
                    if sender.send(WatchEvent::Input(input_event)).is_err() {
                        return;
                    }
                    continue;
                }

                let input_event = match key.code {
                    KeyCode::Char('n') => InputEvent::Next,
                    KeyCode::Char('r') if manual_run => InputEvent::Run,
                    KeyCode::Char('h') => InputEvent::Hint,
                    KeyCode::Char('l') => break WatchEvent::Input(InputEvent::List),
                    KeyCode::Char('m') => break WatchEvent::Input(InputEvent::Mode),
                    KeyCode::Char('t') => InputEvent::Lang,
                    KeyCode::Char('v') => InputEvent::Visuals,
                    KeyCode::Char('d') => InputEvent::Diff,
                    KeyCode::Char('a') => InputEvent::Answer,
                    KeyCode::Char(c @ '1'..='9') => InputEvent::Choose(c as usize - '1' as usize),
                    KeyCode::Enter => InputEvent::Enter,
                    KeyCode::Esc => InputEvent::Esc,
                    KeyCode::Char(c @ ('x' | 'b')) => {
                        let event = if c == 'x' {
                            InputEvent::Reset
                        } else {
                            InputEvent::Baseline
                        };
                        if sender.send(WatchEvent::Input(event)).is_err() {
                            return;
                        }
                        if unpause_receiver.recv().is_err() {
                            return;
                        }
                        continue;
                    }
                    KeyCode::Char('q') => break WatchEvent::Input(InputEvent::Quit),
                    _ => InputEvent::Other,
                };

                if sender.send(WatchEvent::Input(input_event)).is_err() {
                    return;
                }
            }
            Ok(Event::Resize(width, _)) => {
                if sender.send(WatchEvent::TerminalResize { width }).is_err() {
                    return;
                }
            }
            Ok(Event::FocusGained | Event::FocusLost | Event::Mouse(_)) => (),
            Err(e) => break WatchEvent::TerminalEventErr(e),
        }
    };

    let _ = sender.send(last_watch_event);
}
