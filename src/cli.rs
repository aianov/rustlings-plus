use clap::{Parser, Subcommand};

use crate::dev::DevCommand;

/// Rustlings: a course of small exercises, from Rust basics to backend services
#[derive(Parser)]
#[command(version)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,
    /// Disable automatic opening of the current file in VS Code or Zellij.
    /// Ignores `--edit-cmd`
    #[arg(long)]
    pub no_editor: bool,
    /// Open the current exercise by running `EDIT_CMD EXERCISE_PATH`.
    /// The command is not allowed to block (e.g. `vim`).
    /// `EDIT_CMD` can contain arguments like `--edit-cmd "PROGRAM -x --arg1"`.
    /// `--edit-cmd` is ignored in VS Code.
    #[arg(long)]
    pub edit_cmd: Option<String>,
    /// Manually run the current exercise using `r` in the watch mode.
    /// Only use this if Rustlings fails to detect exercise file changes
    #[arg(long)]
    pub manual_run: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Initialize the course in a new `rustlings/` directory
    Init {
        /// Language of the course texts: en or ru
        #[arg(long)]
        lang: Option<String>,
    },
    /// Show the whole course as a tree and pick an exercise
    List,
    /// Run a single exercise.
    /// Runs the current exercise if the exercise name is not specified
    Run {
        /// The name of the exercise
        name: Option<String>,
    },
    /// Check all the exercises, marking them as done or pending accordingly
    CheckAll,
    /// Reset a single exercise (a project folder is recreated)
    Reset {
        /// The name of the exercise
        name: String,
    },
    /// Show a hint.
    /// Shows the hint of the current exercise if the exercise name is not specified
    Hint {
        /// The name of the exercise
        name: Option<String>,
    },
    /// Make an exercise the current one
    Open {
        /// The name of the exercise
        name: String,
    },
    /// Show the tracks or jump to the first unsolved exercise of a track
    Track {
        /// The id of the track
        id: Option<String>,
    },
    /// Answer a question exercise: option numbers (1 2) or the answer text
    Answer {
        /// The name of the question
        name: String,
        /// The answer
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        answer: Vec<String>,
    },
    /// Compare your solution with the reference solution (after solving it)
    Diff {
        /// The name of the exercise
        name: Option<String>,
    },
    /// Spaced repetition: solve a few exercises you solved long ago once more
    Review {
        /// Drop the current review set and pick a new one
        #[arg(long)]
        new: bool,
    },
    /// Check the toolchain, Docker, the course services and the disk space
    Doctor,
    /// Show the modes or switch to one: story, social, sql, cql
    Mode {
        /// The id of the mode
        id: Option<String>,
    },
    /// Show or switch the language (en, ru). Your edits are kept
    Lang {
        /// The new language
        locale: Option<String>,
    },
    /// The end-to-end project: milestones and reference baselines
    #[command(subcommand)]
    Journey(JourneyCommand),
    /// Move an old Rustlings folder into the new course layout (a backup is made first)
    Migrate {
        /// Language of the course texts: en or ru
        #[arg(long)]
        lang: Option<String>,
    },
    /// Commands for developing the course content
    #[command(subcommand)]
    Dev(DevCommand),
}

#[derive(Subcommand)]
pub enum JourneyCommand {
    /// Show the progress of the end-to-end project
    Status,
    /// Start the project from the reference solution of the milestone before the given one
    Baseline {
        /// The milestone you want to work on
        milestone: String,
    },
}
