//! `yp` -- the yourprompt command line.
//!
//! Subcommands land milestone by milestone. Today: `score`, `hook`,
//! `statusline`. Next: `install` (M2), `index` (M3), `bench` (M5).

mod hook;
mod report;
mod session;
mod statusline;

use std::io::{Read, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use report::Style;

#[derive(Parser)]
#[command(
    name = "yp",
    version,
    about = "Score how well an AI coding agent can understand your prompt",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Score a prompt and print the breakdown.
    ///
    /// Reads the prompt from the arguments, or from stdin when none are given.
    Score {
        /// The prompt text. Omit to read from stdin.
        text: Vec<String>,
        /// Emit the full score as JSON instead of a human report.
        #[arg(long)]
        json: bool,
        /// Print only the one-line form, as the status line shows it.
        #[arg(long, conflicts_with = "json")]
        oneline: bool,
        /// Never colourise, even on a terminal.
        #[arg(long)]
        no_color: bool,
    },

    /// Claude Code `UserPromptSubmit` hook. Writes nothing to stdout.
    ///
    /// Reads the hook payload on stdin, scores the prompt, and stores the
    /// result in a per-session sidecar file for the status line to read.
    /// Prints nothing and always exits 0, because a hook's stdout is injected
    /// into the model's context and a non-zero exit can block the prompt.
    Hook,

    /// Render the score for Claude Code's status line.
    ///
    /// Reads the status line payload on stdin and prints one line. Status line
    /// output never reaches the model, which is what keeps this free of
    /// context cost.
    Statusline {
        /// A status line command to run first, whose output ours is appended
        /// to. Lets `yp` be added to an existing status line rather than
        /// replacing it.
        #[arg(long, value_name = "COMMAND")]
        wrap: Option<String>,
    },
}

fn read_stdin() -> std::io::Result<String> {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}

fn run_score(text: Vec<String>, json: bool, oneline: bool, no_color: bool) -> ExitCode {
    let prompt = if text.is_empty() {
        match read_stdin() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("yp: could not read stdin: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        text.join(" ")
    };
    let prompt = prompt.trim();

    let Some(score) = yp_core::score(prompt) else {
        eprintln!("yp: bundled language resources failed to load");
        return ExitCode::FAILURE;
    };

    let style = if no_color {
        Style::plain()
    } else {
        Style::detect()
    };

    let rendered = if json {
        match serde_json::to_string_pretty(&score) {
            Ok(s) => s + "\n",
            Err(e) => {
                eprintln!("yp: could not serialise score: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else if oneline {
        report::one_line(&score, &style) + "\n"
    } else {
        report::full(&score, &style)
    };

    let mut out = std::io::stdout().lock();
    if let Err(e) = out.write_all(rendered.as_bytes()) {
        eprintln!("yp: could not write output: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Score {
            text,
            json,
            oneline,
            no_color,
        } => run_score(text, json, oneline, no_color),
        Command::Hook => hook::run(),
        Command::Statusline { wrap } => statusline::run(wrap),
    }
}
