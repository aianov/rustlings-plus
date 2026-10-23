use anyhow::{Context, Result, bail};
use clap::Subcommand;
use std::{path::Path, process::Command};

use crate::i18n::tr;

const CHECK_PY: &str = "course/tools/check.py";

#[derive(Subcommand)]
pub enum DevCommand {
    /// Run the course content checks (`course/tools/check.py`)
    Check {
        /// Arguments passed to check.py, e.g. `--lint` or `--chapter tokio/01_tasks`
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

impl DevCommand {
    pub fn run(self) -> Result<bool> {
        match self {
            Self::Check { args } => {
                if !Path::new(CHECK_PY).is_file() {
                    bail!("{}", tr().dev_check_missing);
                }
                let status = Command::new("python3")
                    .arg("-I")
                    .arg(CHECK_PY)
                    .args(&args)
                    .status()
                    .context("Failed to run `python3`")?;
                Ok(status.success())
            }
        }
    }
}
