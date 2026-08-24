//! `yp` -- the yourprompt command line.
//!
//! Subcommands land milestone by milestone. Today: `score`. Next: `hook`,
//! `statusline` and `install` (M2), then `index` (M3) and `bench` (M5).

mod report;

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
}

fn read_stdin() -> std::io::Result<String> {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Score {
            text,
            json,
            oneline,
            no_color,
        } => {
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

            let mut out = std::io::stdout().lock();
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

            if let Err(e) = out.write_all(rendered.as_bytes()) {
                eprintln!("yp: could not write output: {e}");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
    }
}
