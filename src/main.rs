use anyhow::{Context, Result, bail};
use app_state::StateFileStatus;
use clap::Parser;
use crossterm::{
    QueueableCommand,
    style::{Color, ResetColor, SetForegroundColor},
};
use std::{
    env,
    io::{self, BufRead, IsTerminal, Write},
    process::ExitCode,
};
use term::{clear_terminal, press_enter_prompt};

use crate::{
    app_state::AppState,
    cli::{Args, Command, JourneyCommand},
    course::{Course, Kind},
    editor::Editor,
    i18n::{Locale, fill, tr},
    run::{current_line, link_line},
    settings::Settings,
};

mod app_state;
mod archcheck;
mod cli;
mod cmd;
mod content_walk;
mod course;
mod dev;
mod diff;
mod doctor;
mod editor;
mod embedded;
mod exercise;
mod fsutil;
mod i18n;
mod init;
mod journey;
mod lang;
mod layout;
mod list;
mod migrate;
mod mode;
mod originals;
mod review;
mod run;
mod services;
mod settings;
mod term;
mod visuals;
mod watch;

fn parse_locale(value: Option<&str>) -> Result<Option<Locale>> {
    value
        .map(|v| Locale::parse(v).with_context(|| fill(tr().unknown_locale, &[&v])))
        .transpose()
}

fn confirm(question: &str) -> Result<bool> {
    if !io::stdin().is_terminal() {
        return Ok(true);
    }
    let mut stdout = io::stdout().lock();
    stdout.write_all(question.as_bytes())?;
    stdout.flush()?;
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "д" | "да"))
}

fn print_migration(migration: &migrate::Migration, course: &Course) -> Result<()> {
    let mut stdout = io::stdout().lock();
    let backup = migration
        .backup
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    writeln!(stdout, "{}", fill(tr().migrate_backup, &[&backup]))?;
    stdout.queue(SetForegroundColor(Color::Green))?;
    write!(stdout, "{}", fill(tr().migrate_done, &[&migration.done]))?;
    stdout.queue(ResetColor)?;
    let current = course
        .find(&migration.current)
        .map_or_else(String::new, |i| {
            exercise::display_path(course, &course.exercises[i])
        });
    writeln!(stdout, "{} ({current})", migration.current)?;
    if !migration.replaced.is_empty() {
        writeln!(
            stdout,
            "{}",
            fill(tr().migrate_replaced, &[&migration.replaced.join(", ")])
        )?;
    }
    if migration.lang.merged + migration.lang.recreated > 0 {
        writeln!(
            stdout,
            "{}",
            fill(
                tr().lang_switched,
                &[
                    &i18n::locale().name(),
                    &migration.lang.recreated,
                    &migration.lang.merged
                ]
            )
        )?;
    }
    Ok(())
}

fn run_migration(course: &'static Course, locale: Locale) -> Result<()> {
    let root = env::current_dir().context("Failed to get the current directory")?;
    println!("{}", tr().migrate_start);
    let migration = migrate::migrate(course, &root, locale)?;
    print_migration(&migration, course)
}

fn main() -> Result<ExitCode> {
    let args = Args::parse();

    let fs = embedded::init()?;
    let early_settings = Settings::read().ok().flatten();
    if let Some(settings) = &early_settings {
        i18n::set_locale(settings.locale);
    }
    let course: &'static Course = Box::leak(Box::new(Course::load(fs)?));
    if early_settings.is_none() {
        i18n::set_locale(course.default_locale);
    }

    let command = match args.command {
        Some(Command::Init { lang }) => {
            init::init(course, parse_locale(lang.as_deref())?).context("Initialization failed")?;
            return Ok(ExitCode::SUCCESS);
        }
        Some(Command::Dev(dev_command)) => {
            return Ok(if dev_command.run()? {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            });
        }
        Some(Command::Doctor) => {
            return Ok(if doctor::run()? {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            });
        }
        Some(Command::Migrate { lang }) => {
            let locale = parse_locale(lang.as_deref())?.unwrap_or(course.default_locale);
            i18n::set_locale(locale);
            run_migration(course, locale)?;
            return Ok(ExitCode::SUCCESS);
        }
        command => command,
    };

    let root = env::current_dir().context("Failed to get the current directory")?;
    let settings = match Settings::read()? {
        Some(settings) => settings,
        None if migrate::is_old_layout(&root) => {
            println!("{}", tr().old_layout);
            let interactive = io::stdin().is_terminal() && io::stdout().is_terminal();
            if !(interactive && confirm(tr().migrate_prompt)?) {
                return Ok(ExitCode::FAILURE);
            }
            run_migration(course, course.default_locale)?;
            Settings::read()?.context("Migration didn't write the settings file")?
        }
        None => {
            println!("{}", tr().pre_init);
            return Ok(ExitCode::FAILURE);
        }
    };
    i18n::set_locale(settings.locale);
    let outdated = layout::sync(course, &root, settings.locale)?;

    let vs_code_term = env::var_os("TERM_PROGRAM").is_some_and(|v| v == "vscode");
    let editor = if args.no_editor {
        None
    } else {
        Editor::new(args.edit_cmd, vs_code_term)?
    };

    let (mut app_state, state_file_status) =
        AppState::new(course, settings.locale, root.clone(), editor, vs_code_term)?;
    review::sync_manifest(course, &root, app_state.extras())?;
    app_state.set_outdated(layout::outdated_exercises(course, &outdated));

    let welcome_message = course.welcome_message.get(settings.locale).trim();
    if !welcome_message.is_empty()
        && matches!(state_file_status, StateFileStatus::NotRead)
        && io::stdout().is_terminal()
    {
        let mut stdout = io::stdout().lock();
        clear_terminal(&mut stdout)?;
        stdout.write_all(welcome_message.as_bytes())?;
        stdout.write_all(b"\n\n")?;
        stdout.write_all(tr().press_enter.as_bytes())?;
        press_enter_prompt(&mut stdout)?;
        clear_terminal(&mut stdout)?;
        stdout.flush()?;
        app_state.save()?;
    }

    let select = |app_state: &mut AppState, name: Option<&str>| -> Result<()> {
        if let Some(name) = name {
            app_state.set_current_exercise_by_name(name)?;
            app_state.open_current(true)?;
            if let Some(notice) = app_state.take_notice() {
                println!("{notice}\n");
            }
        }
        Ok(())
    };

    match command {
        None => {
            if !io::stdout().is_terminal() {
                bail!("{}", tr().need_tty);
            }
            if !mode::choose(&mut app_state, true)? {
                return Ok(ExitCode::SUCCESS);
            }
            app_state.open_current(false)?;
            watch::watch(&mut app_state, args.manual_run)?;
            app_state.close_editor()?;
        }
        Some(Command::List) => {
            if !io::stdout().is_terminal() {
                bail!("{}", tr().need_tty);
            }
            list::list(&mut app_state)?;
            let mut stdout = io::stdout().lock();
            current_line(&mut stdout, tr().current_exercise, &app_state)?;
        }
        Some(Command::Run { name }) => {
            select(&mut app_state, name.as_deref())?;
            if app_state.current_exercise().kind == Kind::Question {
                return run::show_question(&app_state);
            }
            return run::run(&mut app_state);
        }
        Some(Command::Answer { name, answer }) => {
            select(&mut app_state, Some(&name))?;
            return run::answer(&mut app_state, &answer.join(" "));
        }
        Some(Command::Diff { name }) => {
            select(&mut app_state, name.as_deref())?;
            let ind = app_state.current_exercise_ind();
            if !app_state.is_done(ind) {
                println!("{}", tr().diff_after_solve);
                return Ok(ExitCode::FAILURE);
            }
            let files = diff::exercise_diff(app_state.ctx(), ind)?;
            let mut buf = Vec::new();
            diff::render_view(
                &mut buf,
                files.as_deref(),
                0,
                app_state.current_exercise().why.get(app_state.locale()),
            );
            if let Some(files) = files.as_deref() {
                for selected in 1..files.len() {
                    buf.push(b'\n');
                    diff::render_view(&mut buf, Some(&files[selected..=selected]), 0, "");
                }
            }
            io::stdout().lock().write_all(&buf)?;
        }
        Some(Command::Review { new }) => {
            let mut stdout = io::stdout().lock();
            if new {
                app_state.update_extras(|e| e.review = None)?;
            }
            if app_state.extras().review.is_some() {
                return Ok(if review::check(&mut app_state, &mut stdout)? {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                });
            }
            let Some(set) = review::create(&mut app_state)? else {
                writeln!(stdout, "{}", tr().review_none)?;
                return Ok(ExitCode::SUCCESS);
            };
            writeln!(stdout, "{}", fill(tr().review_created, &[&set.date]))?;
            review::print_list(&mut stdout, &set)?;
        }
        Some(Command::CheckAll) => {
            let mut stdout = io::stdout().lock();
            let (first_pending, skipped) = app_state.check_all_with_skipped(&mut stdout)?;
            if !skipped.is_empty() {
                let names = skipped
                    .iter()
                    .map(|&i| app_state.exercise(i).name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                writeln!(stdout, "{}", fill(tr().skipped_in_check_all, &[&names]))?;
            }
            if let Some(first_pending) = first_pending {
                if app_state.is_done(app_state.current_exercise_ind()) {
                    app_state.set_current_exercise_ind(first_pending)?;
                }
                stdout.write_all(b"\n")?;
                let pending = app_state.n_pending();
                if pending == 1 {
                    stdout.write_all(tr().one_pending.as_bytes())?;
                } else {
                    stdout.write_all(
                        fill(tr().many_pending, &[&pending, &app_state.len()]).as_bytes(),
                    )?;
                }
                current_line(&mut stdout, "", &app_state)?;
                return Ok(ExitCode::FAILURE);
            }
            app_state.render_final_message(&mut stdout)?;
        }
        Some(Command::Reset { name }) => {
            app_state.set_current_exercise_by_name(&name)?;
            let ind = app_state.current_exercise_ind();
            let ex = app_state.exercise(ind);
            let path = exercise::display_path(course, ex);
            let question = match ex.kind {
                Kind::File | Kind::Question => fill(tr().reset_file, &[&path]),
                Kind::Project => fill(tr().reset_project, &[&path]),
                Kind::Milestone => fill(tr().reset_milestone, &[&path]),
            };
            if ex.kind != Kind::File && !confirm(&format!("{question}\n{}", tr().yes_no))? {
                return Ok(ExitCode::FAILURE);
            }
            let note = app_state.reset_current_exercise()?;
            let mut stdout = io::stdout().lock();
            if let Some(note) = note {
                writeln!(stdout, "{note}")?;
            }
            link_line(&mut stdout, tr().reset_done, &app_state, ind)?;
            stdout.write_all(b"\n")?;
        }
        Some(Command::Hint { name }) => {
            select(&mut app_state, name.as_deref())?;
            let ind = app_state.current_exercise_ind();
            let opened = app_state.open_next_hint(ind)?;
            let ex = app_state.current_exercise();
            let mut stdout = io::stdout().lock();
            current_line(&mut stdout, tr().current_exercise, &app_state)?;
            if ex.hints(app_state.locale()).len() <= 1 {
                writeln!(stdout, "\n{}:", tr().hint)?;
            } else {
                writeln!(stdout)?;
            }
            watch::write_hints(&mut stdout, ex, app_state.locale(), opened)?;
        }
        Some(Command::Open { name }) => {
            select(&mut app_state, Some(&name))?;
            let mut stdout = io::stdout().lock();
            current_line(&mut stdout, tr().current_exercise, &app_state)?;
            let ex = app_state.current_exercise();
            if let Some(readme) = exercise::task_readme(ex) {
                writeln!(stdout, "{}: {readme}", tr().task)?;
            }
            writeln!(
                stdout,
                "{}: {}",
                tr().theory,
                course.chapter_of(app_state.current_exercise_ind()).readme()
            )?;
        }
        Some(Command::Track { id }) => {
            let locale = app_state.locale();
            let mut stdout = io::stdout().lock();
            let Some(id) = id else {
                writeln!(stdout, "{}", tr().tracks_header)?;
                for (t, track) in course.tracks.iter().enumerate() {
                    let range = app_state.track_range(t);
                    let (done, total) = app_state.range_progress(range.clone());
                    let marker = if range.contains(&app_state.current_exercise_ind()) {
                        ">"
                    } else {
                        " "
                    };
                    stdout.queue(SetForegroundColor(app_state.range_status(range).color()))?;
                    write!(
                        stdout,
                        "{marker} {:<14} {}",
                        track.id,
                        track.title.get(locale)
                    )?;
                    stdout.queue(ResetColor)?;
                    if track.chapters.is_empty() {
                        writeln!(stdout, "  ({})", tr().coming_soon)?;
                    } else {
                        writeln!(stdout, "  {done}/{total}")?;
                    }
                }
                return Ok(ExitCode::SUCCESS);
            };
            let ids = course
                .tracks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let t = course
                .find_track(&id)
                .with_context(|| fill(tr().unknown_track, &[&id, &ids]))?;
            let range = app_state.track_range(t);
            if range.is_empty() {
                bail!("{}", fill(tr().unknown_track, &[&id, &ids]));
            }
            let target = app_state
                .first_pending_in(range.clone())
                .unwrap_or(range.start);
            drop(stdout);
            app_state.set_current_exercise_ind(target)?;
            let mut stdout = io::stdout().lock();
            if let Some(notice) = app_state.take_notice() {
                writeln!(stdout, "{notice}\n")?;
            }
            current_line(&mut stdout, &fill(tr().track_switched, &[&id]), &app_state)?;
        }
        Some(Command::Mode { id }) => {
            let mut stdout = io::stdout().lock();
            let Some(id) = id else {
                writeln!(stdout, "{}", tr().mode_current)?;
                for m in mode::Mode::ALL {
                    let (done, total) = app_state.mode_progress(m);
                    let marker = if m == app_state.mode() { ">" } else { " " };
                    write!(stdout, "{marker} {:<7} {:<26}", m.id(), m.title())?;
                    if total == 0 {
                        writeln!(stdout, "({})", tr().coming_soon)?;
                    } else {
                        writeln!(stdout, "{done}/{total}")?;
                    }
                }
                return Ok(ExitCode::SUCCESS);
            };
            let ids = mode::Mode::ALL.map(mode::Mode::id).join(", ");
            let m =
                mode::Mode::parse(&id).with_context(|| fill(tr().mode_unknown, &[&id, &ids]))?;
            if app_state.mode_progress(m).1 == 0 {
                bail!("{}", tr().mode_empty);
            }
            drop(stdout);
            app_state.set_mode(m)?;
            let mut stdout = io::stdout().lock();
            if let Some(notice) = app_state.take_notice() {
                writeln!(stdout, "{notice}\n")?;
            }
            current_line(
                &mut stdout,
                &fill(tr().mode_switched, &[&m.title()]),
                &app_state,
            )?;
        }
        Some(Command::Lang { locale }) => {
            let Some(locale) = parse_locale(locale.as_deref())? else {
                println!(
                    "{}",
                    fill(tr().lang_current, &[&app_state.locale().as_str()])
                );
                return Ok(ExitCode::SUCCESS);
            };
            let report = app_state.set_locale(locale)?;
            println!(
                "{}",
                fill(
                    tr().lang_switched,
                    &[&locale.name(), &report.recreated, &report.merged]
                )
            );
        }
        Some(Command::Journey(JourneyCommand::Status)) => {
            let locale = app_state.locale();
            let mut stdout = io::stdout().lock();
            for (j, journey) in course.journeys.iter().enumerate() {
                let (done, total, status) = app_state.journey_progress(j);
                stdout.queue(SetForegroundColor(status.color()))?;
                writeln!(
                    stdout,
                    "{} ({}/)",
                    fill(
                        tr().journey_status,
                        &[&journey.title.get(locale), &done, &total]
                    ),
                    journey.dir
                )?;
                stdout.queue(ResetColor)?;
                for (name, ind) in journey.milestones.iter().zip(course.journey_milestones(j)) {
                    let mark = match ind {
                        Some(i) if app_state.is_done(i) => "[x]",
                        Some(_) => "[ ]",
                        None => " - ",
                    };
                    writeln!(stdout, "  {mark} {name}")?;
                }
            }
        }
        Some(Command::Journey(JourneyCommand::Baseline { milestone })) => {
            let ind = course
                .find(&milestone)
                .with_context(|| fill(tr().no_exercise, &[&milestone]))?;
            if course.exercises[ind].kind != Kind::Milestone {
                bail!("{}", fill(tr().not_milestone, &[&milestone]));
            }
            let message = app_state.baseline(ind)?;
            app_state.set_current_exercise_by_name(&milestone)?;
            let mut stdout = io::stdout().lock();
            writeln!(stdout, "{message}")?;
            current_line(&mut stdout, tr().current_exercise, &app_state)?;
        }
        Some(
            Command::Init { .. } | Command::Dev(_) | Command::Migrate { .. } | Command::Doctor,
        ) => (),
    }

    Ok(ExitCode::SUCCESS)
}
