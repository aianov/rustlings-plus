use anyhow::Result;
use crossterm::{
    QueueableCommand,
    style::{Attribute, Color, ResetColor, SetAttribute, SetForegroundColor},
};
use std::{
    io::{self, StdoutLock, Write},
    path::{Path, PathBuf},
};

use crate::{
    archcheck,
    cmd::{CmdRunner, RUN_TIMEOUT},
    course::{Course, Exercise, Kind, package_name},
    fsutil::{remove_dir_if_exists, write_file, write_if_changed},
    i18n::{Locale, fill, tr},
    journey, layout, services,
    term::{self, CountedWrite, file_path, terminal_file_link, write_ansi},
};

pub const OUTPUT_CAPACITY: usize = 1 << 14;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Done,
    Pending,
    Skipped,
}

pub struct Ctx {
    pub course: &'static Course,
    pub locale: Locale,
    pub cmd: CmdRunner,
    pub root: PathBuf,
}

impl Ctx {
    pub fn new(course: &'static Course, locale: Locale, root: PathBuf) -> Self {
        Self {
            course,
            locale,
            cmd: CmdRunner::new(&root),
            root,
        }
    }

    pub fn loc(&self) -> &'static str {
        self.locale.as_str()
    }
}

pub fn display_path(course: &Course, ex: &Exercise) -> String {
    match ex.kind {
        Kind::File => ex.src.clone(),
        Kind::Project => format!("{}/", ex.src),
        Kind::Milestone => ex.journey.map_or_else(
            || ex.src.clone(),
            |j| format!("{}/", course.journeys[j].dir),
        ),
        Kind::Question => ex.src.clone(),
    }
}

pub fn task_readme(ex: &Exercise) -> Option<String> {
    match ex.kind {
        Kind::File | Kind::Question => None,
        Kind::Project | Kind::Milestone => Some(format!("{}/README.md", ex.src)),
    }
}

pub fn editor_path(course: &Course, ex: &Exercise) -> String {
    match ex.kind {
        Kind::File | Kind::Question => ex.src.clone(),
        Kind::Project | Kind::Milestone => {
            task_readme(ex).unwrap_or_else(|| display_path(course, ex))
        }
    }
}

pub fn path_link<'a>(
    writer: &mut impl CountedWrite<'a>,
    path: &str,
    color: Color,
    emit_file_links: bool,
) -> io::Result<()> {
    file_path(writer, color, |writer| {
        if emit_file_links && let Some(canonical) = term::canonicalize(path) {
            terminal_file_link(writer, path, &canonical)
        } else {
            writer.write_str(path)
        }
    })
}

pub fn solution_link_line(
    stdout: &mut StdoutLock,
    solution_path: &str,
    emit_file_links: bool,
) -> io::Result<()> {
    stdout.queue(SetAttribute(Attribute::Bold))?;
    stdout.write_all(tr().solution_for_comparison.as_bytes())?;
    stdout.queue(ResetColor)?;
    path_link(stdout, solution_path, Color::Cyan, emit_file_links)?;
    stdout.write_all(b"\n")
}

fn note(output: Option<&mut Vec<u8>>, color: Color, text: &str) {
    if let Some(output) = output {
        write_ansi(output, SetAttribute(Attribute::Bold));
        write_ansi(output, SetForegroundColor(color));
        output.extend_from_slice(text.as_bytes());
        write_ansi(output, ResetColor);
        output.push(b'\n');
    }
}

pub fn tidy_paths(output: &mut Vec<u8>, needle: &[u8]) {
    if needle.is_empty() || output.len() < needle.len() {
        return;
    }
    let mut out = Vec::with_capacity(output.len());
    let mut i = 0;
    while i < output.len() {
        if output[i..].starts_with(needle) {
            i += needle.len();
        } else {
            out.push(output[i]);
            i += 1;
        }
    }
    *output = out;
}

fn clear(output: Option<&mut Vec<u8>>) {
    if let Some(output) = output {
        output.clear();
    }
}

fn run_bin(ctx: &Ctx, name: &str, cwd: &Path, mut output: Option<&mut Vec<u8>>) -> Result<bool> {
    if let Some(output) = output.as_deref_mut() {
        write_ansi(output, SetAttribute(Attribute::Underlined));
        output.extend_from_slice(tr().output.as_bytes());
        write_ansi(output, ResetColor);
        output.push(b'\n');
    }
    let success = ctx.cmd.run_debug_bin(name, cwd, output.as_deref_mut())?;
    if !success {
        note(output, Color::Red, tr().nonzero_exit);
    }
    Ok(success)
}

pub struct FileTarget {
    pub package: String,
    pub bin: String,
    pub cwd: PathBuf,
    pub test: bool,
    pub strict_clippy: bool,
    pub inputs: Vec<(PathBuf, &'static [u8])>,
    pub tidy: String,
}

impl FileTarget {
    pub fn of(ctx: &Ctx, ex: &Exercise) -> Self {
        let course = ctx.course;
        let chapter_dir = course.chapters[ex.chapter].exercises_dir();
        let track = &course.tracks[ex.track].id;
        Self {
            package: package_name(track),
            bin: ex.name.clone(),
            cwd: ctx.root.join(&chapter_dir),
            test: ex.test,
            strict_clippy: ex.strict_clippy,
            inputs: ex
                .input_files
                .iter()
                .filter_map(|input| {
                    let src = format!("{chapter_dir}/{input}");
                    course
                        .fs
                        .resolve(&src, ctx.loc())
                        .map(|content| (ctx.root.join(&src), content))
                })
                .collect(),
            tidy: format!("tracks/{track}/../../"),
        }
    }
}

fn run_file_steps(
    ctx: &Ctx,
    target: &FileTarget,
    mut output: Option<&mut Vec<u8>>,
) -> Result<Outcome> {
    for (path, content) in &target.inputs {
        write_if_changed(path, content)?;
    }

    let package = target.package.as_str();
    let name = target.bin.as_str();
    let cmd = &ctx.cmd;

    if !cmd
        .cargo("build", package, name, output.as_deref_mut())
        .run("cargo build …")?
    {
        return Ok(Outcome::Pending);
    }
    clear(output.as_deref_mut());

    if target.test {
        let mut compile = cmd.cargo("test", package, name, output.as_deref_mut());
        compile.args(["--no-run"]);
        if !compile.run("cargo test …")? {
            return Ok(Outcome::Pending);
        }
        clear(output.as_deref_mut());

        let output_is_some = output.is_some();
        let mut test_cmd = cmd.cargo("test", package, name, output.as_deref_mut());
        test_cmd.timeout(RUN_TIMEOUT);
        if output_is_some {
            test_cmd.args(["--", "--color", "always", "--format", "pretty"]);
        }
        if !test_cmd.run("cargo test …")? {
            run_bin(ctx, name, &target.cwd, output)?;
            return Ok(Outcome::Pending);
        }
        clear(output.as_deref_mut());
    }

    let mut clippy = cmd.cargo("clippy", package, name, output.as_deref_mut());
    if target.strict_clippy {
        clippy.args(["--profile", "test", "--", "-D", "warnings"]);
    } else {
        clippy.args(["--profile", "test"]);
    }
    let clippy_success = clippy.run("cargo clippy …")?;
    let run_success = run_bin(ctx, name, &target.cwd, output)?;

    Ok(if clippy_success && run_success {
        Outcome::Done
    } else {
        Outcome::Pending
    })
}

pub fn run_file_target(
    ctx: &Ctx,
    target: &FileTarget,
    mut output: Option<&mut Vec<u8>>,
) -> Result<Outcome> {
    let outcome = run_file_steps(ctx, target, output.as_deref_mut());
    if let Some(output) = output {
        tidy_paths(output, target.tidy.as_bytes());
    }
    outcome
}

pub fn materialize_project(ctx: &Ctx, ex: &Exercise) -> Result<()> {
    let dir = ctx.root.join(&ex.src);
    remove_dir_if_exists(&dir)?;
    let files = layout::project_files(ctx.course, &ex.src, ctx.locale);
    for (path, content) in &files {
        write_file(ctx.root.join(path), content)?;
    }
    crate::originals::record(&ctx.root, files.iter().map(|(p, c)| (p.as_str(), *c)))
}

pub fn restore_locked(ctx: &Ctx, ex: &Exercise) -> Result<()> {
    for locked in &ex.locked {
        let src = format!("{}/{locked}", ex.src);
        if let Some(content) = ctx.course.fs.resolve(&src, ctx.loc()) {
            write_if_changed(ctx.root.join(&src), content)?;
        }
    }
    Ok(())
}

fn run_project(ctx: &Ctx, ex: &Exercise, mut output: Option<&mut Vec<u8>>) -> Result<Outcome> {
    let dir = ctx.root.join(&ex.src);
    let mut missing_note = None;
    if !dir.is_dir() {
        materialize_project(ctx, ex)?;
        missing_note = Some(fill(tr().project_missing, &[&display_path(ctx.course, ex)]));
    }
    restore_locked(ctx, ex)?;
    let success = ctx
        .cmd
        .project_command(&ex.test_command, &dir, output.as_deref_mut())?;
    if let Some(text) = missing_note {
        note(output, Color::Yellow, &text);
    }
    Ok(if success {
        Outcome::Done
    } else {
        Outcome::Pending
    })
}

fn run_milestone(ctx: &Ctx, ind: usize, mut output: Option<&mut Vec<u8>>) -> Result<Outcome> {
    let course = ctx.course;
    let ex = &course.exercises[ind];
    let Some(dir) = journey::dir_of(course, ex) else {
        return Ok(Outcome::Pending);
    };
    let dir = ctx.root.join(dir);
    if !dir.is_dir() {
        if journey::is_first(course, ind) {
            journey::apply_scaffold(ctx, ind)?;
        } else {
            note(
                output.as_deref_mut(),
                Color::Yellow,
                &fill(tr().journey_missing, &[&display_path(course, ex)]),
            );
            if let Some(text) = journey::baseline_offer(course, ind, false) {
                note(output, Color::Yellow, &text);
            }
            return Ok(Outcome::Pending);
        }
    }

    if let Some(later) = journey::ahead_of(ctx, ind) {
        note(
            output,
            Color::Yellow,
            &journey::ahead_notice(course, ind, later, false),
        );
        return Ok(Outcome::Skipped);
    }

    journey::write_inject(ctx, ind)?;

    if let archcheck::Report::Violations(list) = archcheck::check_milestone(ctx, ind)? {
        note(output.as_deref_mut(), Color::Red, tr().arch_failed);
        if let Some(output) = output {
            for v in list {
                output.extend_from_slice(b"  - ");
                write_ansi(output, SetAttribute(Attribute::Bold));
                output.extend_from_slice(v.rule.as_bytes());
                write_ansi(output, ResetColor);
                if let Some(path) = &v.path {
                    output.extend_from_slice(b" ");
                    write_ansi(output, SetForegroundColor(Color::Blue));
                    output.extend_from_slice(path.as_bytes());
                    write_ansi(output, ResetColor);
                }
                output.extend_from_slice(b": ");
                output.extend_from_slice(v.message.as_bytes());
                output.push(b'\n');
            }
        }
        return Ok(Outcome::Pending);
    }

    let success = ctx.cmd.project_command(&ex.test_command, &dir, output)?;
    Ok(if success {
        Outcome::Done
    } else {
        Outcome::Pending
    })
}

pub fn run_exercise(ctx: &Ctx, ind: usize, mut output: Option<&mut Vec<u8>>) -> Result<Outcome> {
    clear(output.as_deref_mut());
    let ex = &ctx.course.exercises[ind];
    if let Some(text) = services::down_message(&ex.requires) {
        note(output, Color::Yellow, &text);
        return Ok(Outcome::Skipped);
    }
    match ex.kind {
        Kind::File => run_file_target(ctx, &FileTarget::of(ctx, ex), output),
        Kind::Project => run_project(ctx, ex, output),
        Kind::Milestone => run_milestone(ctx, ind, output),
        Kind::Question => {
            note(output, Color::Yellow, &fill(tr().question_cli, &[&ex.name]));
            Ok(Outcome::Skipped)
        }
    }
}

pub fn write_solution(ctx: &Ctx, ind: usize) -> Result<Option<String>> {
    let course = ctx.course;
    let ex = &course.exercises[ind];
    match ex.kind {
        Kind::Question => Ok(None),
        Kind::File => {
            let Some(content) = course.fs.resolve(&ex.sol, ctx.loc()) else {
                return Ok(None);
            };
            write_if_changed(ctx.root.join(&ex.sol), content)?;
            Ok(Some(ex.sol.clone()))
        }
        Kind::Project => {
            let files = layout::project_files(course, &ex.sol, ctx.locale);
            if files.is_empty() {
                return Ok(None);
            }
            let dir = ctx.root.join(&ex.sol);
            remove_dir_if_exists(&dir)?;
            for (path, content) in files {
                write_file(ctx.root.join(path), content)?;
            }
            Ok(Some(format!("{}/", ex.sol)))
        }
        Kind::Milestone => {
            let Some(snapshot) = journey::snapshot_upto(ctx, ind)? else {
                return Ok(None);
            };
            let dir = ctx.root.join(&ex.sol);
            remove_dir_if_exists(&dir)?;
            for (path, content) in snapshot {
                write_file(dir.join(path), content)?;
            }
            Ok(Some(format!("{}/", ex.sol)))
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn tidy() {
        let mut out = b"--> tracks/t/../../exercises/t/a.rs:1\n tracks/t/../../x".to_vec();
        super::tidy_paths(&mut out, b"tracks/t/../../");
        assert_eq!(out, b"--> exercises/t/a.rs:1\n x");
    }
}
