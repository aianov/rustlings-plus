use anyhow::{Result, bail};
use crossterm::{
    QueueableCommand,
    style::{Attribute, Color, ResetColor, SetAttribute, SetForegroundColor},
};
use std::{
    io::{self, StdoutLock, Write},
    process::ExitCode,
};

use crate::{
    app_state::{AppState, ExercisesProgress},
    course::Kind,
    exercise::{self, OUTPUT_CAPACITY, Outcome, path_link, solution_link_line},
    i18n::{fill, split2, tr},
};

pub fn link_line(
    stdout: &mut StdoutLock,
    template: &str,
    app_state: &AppState,
    ind: usize,
) -> io::Result<()> {
    let (before, after) = split2(template);
    stdout.write_all(before.as_bytes())?;
    let ex = app_state.exercise(ind);
    let path = exercise::display_path(app_state.course(), ex);
    path_link(stdout, &path, Color::Blue, app_state.emit_file_links())?;
    stdout.write_all(after.as_bytes())
}

pub fn current_line(stdout: &mut StdoutLock, label: &str, app_state: &AppState) -> io::Result<()> {
    stdout.write_all(label.as_bytes())?;
    let ex = app_state.current_exercise();
    stdout.write_all(ex.name.as_bytes())?;
    stdout.write_all(b" (")?;
    let path = exercise::display_path(app_state.course(), ex);
    path_link(stdout, &path, Color::Blue, app_state.emit_file_links())?;
    stdout.write_all(b")\n")
}

fn celebrate(stdout: &mut StdoutLock, app_state: &AppState) -> io::Result<()> {
    let ex = app_state.current_exercise();
    let locale = app_state.locale();
    stdout.queue(SetForegroundColor(Color::Magenta))?;
    writeln!(stdout, "\n{}", fill(tr().milestone_done, &[&ex.name]))?;
    stdout.queue(ResetColor)?;
    let story = ex.story.get(locale).trim();
    if !story.is_empty() {
        writeln!(stdout, "{story}")?;
    }
    if let Some(j) = ex.journey {
        let (done, total, _) = app_state.journey_progress(j);
        let title = app_state.course().journeys[j].title.get(locale);
        writeln!(
            stdout,
            "{}",
            fill(tr().journey_status, &[&title, &done, &total])
        )?;
    }
    Ok(())
}

fn write_question(stdout: &mut StdoutLock, app_state: &AppState) -> io::Result<()> {
    let ex = app_state.current_exercise();
    let locale = app_state.locale();
    let q = &ex.question;
    stdout.queue(SetAttribute(Attribute::Bold))?;
    writeln!(stdout, "{}: {}", tr().question, ex.name)?;
    stdout.queue(ResetColor)?;
    writeln!(stdout, "{}\n", q.text.get(locale).trim())?;
    for (i, option) in q.options.get(locale).iter().enumerate() {
        writeln!(stdout, "  {}) {}", i + 1, option.trim())?;
    }
    Ok(())
}

pub fn show_question(app_state: &AppState) -> Result<ExitCode> {
    let mut stdout = io::stdout().lock();
    write_question(&mut stdout, app_state)?;
    let ex = app_state.current_exercise();
    if app_state.is_done(app_state.current_exercise_ind()) {
        let explain = ex.question.explain.get(app_state.locale()).trim();
        if !explain.is_empty() {
            writeln!(stdout, "\n{explain}")?;
        }
        return Ok(ExitCode::SUCCESS);
    }
    writeln!(stdout, "\n{}", fill(tr().question_cli, &[&ex.name]))?;
    Ok(ExitCode::FAILURE)
}

pub fn answer(app_state: &mut AppState, given: &str) -> Result<ExitCode> {
    let ex = app_state.current_exercise();
    if ex.kind != Kind::Question {
        bail!("{}", fill(tr().not_question, &[&ex.name]));
    }
    let q = &ex.question;
    let (right, stored) = if q.is_free_text() {
        (q.check_text(given), given.trim().to_owned())
    } else {
        let chosen = given
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .filter(|n| *n > 0)
            .map(|n| n - 1)
            .collect::<Vec<_>>();
        let stored = chosen
            .iter()
            .map(|i| (i + 1).to_string())
            .collect::<Vec<_>>()
            .join(",");
        (q.check_options(&chosen), stored)
    };
    let name = ex.name.clone();
    app_state.update_extras(|e| {
        e.answers.insert(name, stored);
    })?;

    let mut stdout = io::stdout().lock();
    stdout
        .queue(SetAttribute(Attribute::Bold))?
        .queue(SetForegroundColor(if right {
            Color::Green
        } else {
            Color::Red
        }))?;
    writeln!(
        stdout,
        "{}",
        if right {
            tr().answer_right
        } else {
            tr().answer_wrong
        }
    )?;
    stdout.queue(ResetColor)?;
    let explain = q.explain.get(app_state.locale()).trim();
    if !explain.is_empty() {
        writeln!(stdout, "{explain}")?;
    }
    if !right {
        return Ok(ExitCode::FAILURE);
    }
    match app_state.done_current_exercise::<false>(&mut stdout)? {
        ExercisesProgress::NewPending | ExercisesProgress::CurrentPending => {
            current_line(&mut stdout, tr().next_exercise, app_state)?;
        }
        ExercisesProgress::AllDone => (),
    }
    Ok(ExitCode::SUCCESS)
}

pub fn run(app_state: &mut AppState) -> Result<ExitCode> {
    let ind = app_state.current_exercise_ind();
    let mut output = Vec::with_capacity(OUTPUT_CAPACITY);
    let outcome = exercise::run_exercise(app_state.ctx(), ind, Some(&mut output))?;

    let mut stdout = io::stdout().lock();
    stdout.write_all(&output)?;

    match outcome {
        Outcome::Pending => {
            app_state.set_pending(ind)?;
            link_line(&mut stdout, tr().ran_with_errors, app_state, ind)?;
            stdout.write_all(b"\n")?;
            return Ok(ExitCode::FAILURE);
        }
        Outcome::Skipped => {
            link_line(&mut stdout, tr().skipped, app_state, ind)?;
            stdout.write_all(b"\n")?;
            return Ok(ExitCode::FAILURE);
        }
        Outcome::Done => (),
    }

    stdout.queue(SetForegroundColor(Color::Green))?;
    let (before, after) = split2(tr().ran_ok);
    stdout.write_all(before.as_bytes())?;
    stdout.write_all(
        exercise::display_path(app_state.course(), app_state.current_exercise()).as_bytes(),
    )?;
    stdout.write_all(after.as_bytes())?;
    stdout.queue(ResetColor)?;
    stdout.write_all(b"\n")?;

    if app_state.current_exercise().kind == Kind::Milestone {
        celebrate(&mut stdout, app_state)?;
    }

    if let Some(solution_path) = app_state.write_current_solution()? {
        stdout.write_all(b"\n")?;
        solution_link_line(&mut stdout, &solution_path, app_state.emit_file_links())?;
        stdout.write_all(b"\n")?;
    }

    match app_state.done_current_exercise::<false>(&mut stdout)? {
        ExercisesProgress::NewPending | ExercisesProgress::CurrentPending => {
            current_line(&mut stdout, tr().next_exercise, app_state)?;
        }
        ExercisesProgress::AllDone => (),
    }

    Ok(ExitCode::SUCCESS)
}
