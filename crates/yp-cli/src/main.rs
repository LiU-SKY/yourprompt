//! `yp` -- the yourprompt command line.
//!
//! Subcommands land milestone by milestone. Today: `score`, `hook`,
//! `statusline`, `install`, `explain`, `index`. Next: `bench` (M5).

mod explain;
mod hook;
mod install;
mod repo;
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

    /// Build the repository index the grounding axis needs.
    ///
    /// Never run by the hook: indexing a large repository takes seconds and
    /// the hook must not delay a prompt. Run it once per session (the plugin
    /// does this at session start) or after large changes.
    Index {
        /// Repository to index. Defaults to the current directory.
        #[arg(long, value_name = "PATH")]
        root: Option<String>,
        /// Skip the rebuild if the existing index is younger than this many
        /// seconds. Lets a session-start hook run unconditionally and be
        /// cheap when nothing has changed.
        #[arg(long, value_name = "SECONDS")]
        max_age: Option<u64>,
        /// Print nothing on success.
        #[arg(long)]
        quiet: bool,
    },

    /// Show the full breakdown of the prompt that was just scored.
    ///
    /// Backs the `/score` slash command. With no --session, uses the most
    /// recently scored session, preferring one from this directory.
    Explain {
        /// Explain a specific session rather than the most recent one.
        #[arg(long, value_name = "ID")]
        session: Option<String>,
        /// Never colourise, even on a terminal.
        #[arg(long)]
        no_color: bool,
    },

    /// Register the hook and status line in Claude Code's settings.
    ///
    /// Backs up your settings first, and wraps any status line you already
    /// have rather than replacing it.
    Install {
        /// Print the settings that would be written, without writing them.
        #[arg(long)]
        print_only: bool,
        /// Undo a previous install, restoring any status line that was wrapped.
        #[arg(long)]
        uninstall: bool,
    },

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

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let corpus = repo::load_for(&cwd);
    let Some(score) =
        yp_core::score_with(prompt, corpus.as_ref().map(|c| c as &dyn yp_core::Corpus))
    else {
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

fn run_index(root: Option<String>, max_age: Option<u64>, quiet: bool) -> ExitCode {
    let start = match root {
        Some(path) => std::path::PathBuf::from(path),
        None => std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
    };

    if let (Some(max_age), Some(age)) = (max_age, repo::age(&start)) {
        if age.as_secs() < max_age {
            if !quiet {
                println!("index is {}s old; skipping rebuild", age.as_secs());
            }
            return ExitCode::SUCCESS;
        }
    }

    match repo::build_for(&start) {
        Ok((root, files, terms)) => {
            if !quiet {
                println!(
                    "indexed {files} files, {terms} distinct terms, from {}",
                    root.display()
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            // Indexing failing must never be fatal for the caller -- a
            // session-start hook that returns non-zero would surface an error
            // to the user over a cache that simply is not there yet.
            if !quiet {
                eprintln!("yp: could not build the index: {e}");
            }
            ExitCode::SUCCESS
        }
    }
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
        Command::Explain { session, no_color } => explain::run(session, no_color),
        Command::Index {
            root,
            max_age,
            quiet,
        } => run_index(root, max_age, quiet),
        Command::Hook => hook::run(),
        Command::Install {
            print_only,
            uninstall,
        } => install::run(print_only, uninstall),
        Command::Statusline { wrap } => statusline::run(wrap),
    }
}
