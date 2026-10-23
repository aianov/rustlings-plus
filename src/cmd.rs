use anyhow::{Context, Result, bail};
use std::{
    io::{Read, pipe},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};
use wait_timeout::ChildExt;

use crate::i18n::{fill, tr};

pub const BUILD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
pub const RUN_TIMEOUT: Duration = Duration::from_secs(60);
pub const PROJECT_TEST_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub fn run_cmd(
    mut cmd: Command,
    description: &str,
    output: Option<&mut Vec<u8>>,
    timeout: Duration,
) -> Result<bool> {
    let spawn = |mut cmd: Command| {
        cmd.stdin(Stdio::null())
            .spawn()
            .with_context(|| fill(tr().run_failed, &[&description]))
    };
    let wait = |handle: &mut Child| {
        handle
            .wait_timeout(timeout)
            .with_context(|| format!("Failed to wait on `{description}` to exit"))
    };

    let mut handle = if let Some(output) = output {
        let (mut reader, writer) =
            pipe().with_context(|| format!("Failed to create a pipe to run `{description}`"))?;
        let writer_clone = writer
            .try_clone()
            .with_context(|| format!("Failed to clone the pipe writer for `{description}`"))?;
        cmd.stdout(writer_clone).stderr(writer);
        let mut handle = spawn(cmd)?;

        let thread_handle = thread::Builder::new()
            .spawn(move || {
                let mut out = Vec::with_capacity(128);
                reader.read_to_end(&mut out).map(|_| out)
            })
            .context("Failed to spawn a thread to collect a command's output")?;

        if let Some(status) = wait(&mut handle)? {
            let out = thread_handle
                .join()
                .unwrap()
                .with_context(|| format!("Failed to read the output of `{description}`"))?;
            output.extend_from_slice(&out);
            output.push(b'\n');
            return Ok(status.success());
        }

        handle
    } else {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        let mut handle = spawn(cmd)?;
        if let Some(status) = wait(&mut handle)? {
            return Ok(status.success());
        }
        handle
    };

    handle
        .kill()
        .with_context(|| format!("Failed to kill `{description}` after timeout"))?;
    let _ = handle.wait();
    bail!(
        "{}",
        fill(tr().timed_out, &[&description, &timeout.as_secs()])
    );
}

pub struct CmdRunner {
    target_dir: PathBuf,
}

impl CmdRunner {
    pub fn new(root: &Path) -> Self {
        Self {
            target_dir: root.join("target"),
        }
    }

    pub fn cargo<'out>(
        &self,
        subcommand: &str,
        package: &str,
        bin_name: &str,
        output: Option<&'out mut Vec<u8>>,
    ) -> CargoSubcommand<'out> {
        let mut cmd = Command::new("cargo");
        cmd.arg(subcommand)
            .arg("-q")
            .arg("-p")
            .arg(package)
            .arg("--bin")
            .arg(bin_name)
            .env("CARGO_TARGET_DIR", &self.target_dir);
        if output.is_some() {
            cmd.arg("--color").arg("always");
        }
        CargoSubcommand {
            cmd,
            output,
            timeout: BUILD_TIMEOUT,
        }
    }

    pub fn run_debug_bin(
        &self,
        bin_name: &str,
        cwd: &Path,
        output: Option<&mut Vec<u8>>,
    ) -> Result<bool> {
        let mut bin_path = self.target_dir.join("debug");
        bin_path.push(format!("{bin_name}{}", std::env::consts::EXE_SUFFIX));
        let mut cmd = Command::new(&bin_path);
        cmd.current_dir(cwd);
        run_cmd(cmd, bin_name, output, RUN_TIMEOUT)
    }

    pub fn project_command(
        &self,
        argv: &[String],
        cwd: &Path,
        mut output: Option<&mut Vec<u8>>,
    ) -> Result<bool> {
        let description = argv.join(" ");
        let make = |args: &[String]| {
            let mut cmd = Command::new(&args[0]);
            cmd.args(&args[1..])
                .current_dir(cwd)
                .env("CARGO_TARGET_DIR", &self.target_dir);
            cmd
        };
        let colored = output.is_some();
        let with_color = |mut cmd: Command| {
            if colored {
                cmd.env("CARGO_TERM_COLOR", "always");
            }
            cmd
        };

        if let Some(compile) = compile_step(argv) {
            if !run_cmd(
                with_color(make(&compile)),
                &description,
                output.as_deref_mut(),
                BUILD_TIMEOUT,
            )? {
                return Ok(false);
            }
            if let Some(output) = output.as_deref_mut() {
                output.clear();
            }
        }

        run_cmd(
            with_color(make(argv)),
            &description,
            output,
            PROJECT_TEST_TIMEOUT,
        )
    }
}

fn compile_step(argv: &[String]) -> Option<Vec<String>> {
    if argv.len() < 2 || argv[0] != "cargo" || argv[1] != "test" {
        return None;
    }
    let mut compile = Vec::with_capacity(argv.len() + 1);
    let mut inserted = false;
    for arg in argv {
        if !inserted && arg == "--" {
            compile.push("--no-run".to_owned());
            inserted = true;
        }
        if inserted && arg == "--" {
            break;
        }
        compile.push(arg.clone());
    }
    if !inserted {
        compile.push("--no-run".to_owned());
    }
    Some(compile)
}

pub struct CargoSubcommand<'out> {
    cmd: Command,
    output: Option<&'out mut Vec<u8>>,
    timeout: Duration,
}

impl CargoSubcommand<'_> {
    pub fn args<'arg, I>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = &'arg str>,
    {
        self.cmd.args(args);
        self
    }

    pub fn timeout(&mut self, timeout: Duration) -> &mut Self {
        self.timeout = timeout;
        self
    }

    pub fn run(self, description: &str) -> Result<bool> {
        run_cmd(self.cmd, description, self.output, self.timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_cmd() {
        let mut cmd = Command::new("echo");
        cmd.arg("Hello");
        let mut output = Vec::with_capacity(8);
        run_cmd(cmd, "echo …", Some(&mut output), RUN_TIMEOUT).unwrap();
        assert_eq!(output, b"Hello\n\n");
    }

    #[test]
    fn compile_steps() {
        let v = |s: &[&str]| s.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            compile_step(&v(&["cargo", "test", "--workspace"])),
            Some(v(&["cargo", "test", "--workspace", "--no-run"]))
        );
        assert_eq!(
            compile_step(&v(&["cargo", "test", "-p", "x", "--", "--nocapture"])),
            Some(v(&["cargo", "test", "-p", "x", "--no-run"]))
        );
        assert_eq!(compile_step(&v(&["make", "test"])), None);
        assert_eq!(compile_step(&v(&["cargo", "run"])), None);
    }
}
