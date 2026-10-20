//! `yp` -- the yourprompt command line.
//!
//! Subcommands land milestone by milestone. Today: `score`, `hook`,
//! `statusline`, `install`, `explain`, `index`, `bench`, `serve`.

mod bench;
mod explain;
mod hook;
mod install;
mod repo;
mod report;
mod serve;
mod session;
mod statusline;

use std::io::{IsTerminal, Read, Write};
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
        /// Ground against a specific index file instead of this directory's.
        ///
        /// Mostly a debugging aid: it answers "what would this prompt score
        /// against *that* codebase", which is how the grounding axis gets
        /// checked.
        #[arg(long, value_name = "PATH")]
        index: Option<String>,
    },

    /// Claude Code `UserPromptSubmit` hook. Writes nothing to stdout.
    ///
    /// Reads the hook payload on stdin, scores the prompt, and stores the
    /// result in a per-session sidecar file for the status line to read.
    /// Prints nothing and always exits 0, because a hook's stdout is injected
    /// into the model's context and a non-zero exit can block the prompt.
    Hook,

    /// Check the score against published ambiguity benchmarks.
    ///
    /// Downloads HumanEvalComm on first use and caches it, then reports how
    /// often the score ranks an original specification above a deliberately
    /// damaged version of itself.
    Bench {
        /// Which check to run. `humanevalcomm` asks whether the score falls
        /// when a specification is damaged; `swe` asks whether the grounding
        /// axis is actually reading the repository.
        #[arg(long, value_name = "NAME", default_value = "humanevalcomm")]
        dataset: String,
        /// swe only: how many repositories to download and compare across.
        #[arg(long, value_name = "N", default_value_t = 6)]
        repos: usize,
        /// Re-download the dataset even if it is cached.
        #[arg(long)]
        refresh: bool,
        /// Also report each axis's contribution by removing it in turn.
        #[arg(long)]
        ablation: bool,
        /// Use only the first N problems. Useful for a quick check.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
        /// Write the report to a file instead of stdout.
        #[arg(long, value_name = "PATH")]
        report: Option<String>,
        /// swe only: also write one tab-separated row per issue, for
        /// diagnosing where a number comes from.
        #[arg(long, value_name = "PATH")]
        dump: Option<String>,
    },

    /// Build the repository index the grounding axis needs.
    ///
    /// Never run by the hook: indexing a large repository takes seconds and
    /// the hook must not delay a prompt. Run it once per session (the hooks
    /// `yp install` and the plugin register do this at session start, in
    /// the background) or after large changes.
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
    /// Backs the `/score` slash command. With no --session, explains the
    /// session in CLAUDE_CODE_SESSION_ID, which Claude Code sets for the
    /// commands it runs; outside Claude Code, the most recently scored
    /// session, preferring one from this directory.
    Explain {
        /// Explain a specific session rather than the current one.
        #[arg(long, value_name = "ID")]
        session: Option<String>,
        /// Never colourise, even on a terminal.
        #[arg(long)]
        no_color: bool,
    },

    /// Register the hooks, the status line and /score with Claude Code.
    ///
    /// Backs up your settings the first time it changes them, and wraps any
    /// status line you already have rather than replacing it.
    Install {
        /// Print the settings that would be written, without writing them.
        #[arg(long)]
        print_only: bool,
        /// Undo a previous install, restoring any status line that was wrapped.
        #[arg(long)]
        uninstall: bool,
    },

    /// Serve the score in a browser.
    ///
    /// A small local web page: type a prompt, watch it scored as you go.
    /// Binds to loopback unless told otherwise.
    Serve {
        /// Address to bind. Use 0.0.0.0 to accept connections from the
        /// network -- only on a network you trust.
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
        /// Port to listen on.
        #[arg(long, default_value_t = 8787)]
        port: u16,
        /// Index this repository at startup and ground every score in it.
        #[arg(long, value_name = "PATH")]
        repo: Option<String>,
    },

    /// Render the score for Claude Code's status line.
    ///
    /// Reads the status line payload on stdin and prints one line. Status line
    /// output never reaches the model, which is what keeps this free of
    /// context cost.
    Statusline {
        /// Run the status line `yp install` found and kept, and append ours
        /// to its output.
        #[arg(long, conflicts_with = "wrap")]
        wrapped: bool,
        /// A status line command to run first, whose output ours is appended
        /// to. What earlier installs wrote; `yp install` now keeps the
        /// command out of the command line and passes --wrapped instead.
        #[arg(long, value_name = "COMMAND")]
        wrap: Option<String>,
    },
}

fn read_stdin() -> std::io::Result<String> {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    Ok(without_bom(buf))
}

/// Windows PowerShell 5.1 puts a byte order mark in front of what it pipes
/// to a program, so `'/score' | yp score` was graded as a task: the prompt
/// began with U+FEFF, not with the slash. `trim` leaves it, as it is not
/// white space. With the console on UTF-8 and `$OutputEncoding` set to
/// `[Text.Encoding]::UTF8` it puts two, and '/score' scored 15.1 F.
fn without_bom(text: String) -> String {
    match text.trim_start_matches('\u{feff}') {
        rest if rest.len() == text.len() => text,
        rest => rest.to_string(),
    }
}

fn run_score(
    text: Vec<String>,
    json: bool,
    oneline: bool,
    no_color: bool,
    index: Option<String>,
) -> ExitCode {
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

    let corpus = match &index {
        Some(path) => {
            let corpus = repo::load_index_at(std::path::Path::new(path));
            if corpus.is_none() {
                eprintln!("yp: could not read an index at {path}");
                return ExitCode::FAILURE;
            }
            corpus
        }
        None => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            repo::load_for(&cwd)
        }
    };
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
    // `yp install` writes its hooks in the exec form, which Claude Code
    // 2.1.139 introduced. One older than that, if it skips the `args` it does
    // not know, runs the executable alone: plain `yp`, with the hook payload
    // on stdin. clap's usage error exits 2, and exit 2 from a
    // UserPromptSubmit hook blocks the prompt. So with no arguments and no
    // one at a terminal to read the usage, say what is missing on stderr,
    // which `claude --debug` shows, and exit 0.
    if std::env::args_os().len() <= 1 && !std::io::stdin().is_terminal() {
        eprintln!(
            "yp: no command given (`yp --help` lists them); a hook that runs yp \
             needs Claude Code 2.1.139 or newer to pass its arguments"
        );
        return ExitCode::SUCCESS;
    }

    let cli = Cli::parse();
    match cli.command {
        Command::Score {
            text,
            json,
            oneline,
            no_color,
            index,
        } => run_score(text, json, oneline, no_color, index),
        Command::Bench {
            dataset,
            repos,
            refresh,
            ablation,
            limit,
            report,
            dump,
        } => match dataset.as_str() {
            "humanevalcomm" => bench::humanevalcomm::run(refresh, ablation, limit, report),
            "swe" => bench::swe::run(repos, limit, refresh, report, dump),
            other => {
                eprintln!("yp: unknown dataset {other:?}; expected humanevalcomm or swe");
                ExitCode::FAILURE
            }
        },
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
        Command::Serve { bind, port, repo } => serve::run(bind, port, repo),
        Command::Statusline { wrapped, wrap } => statusline::run(wrap, wrapped),
    }
}

#[cfg(test)]
mod tests {
    use super::without_bom;

    #[test]
    fn a_byte_order_mark_in_front_of_piped_text_is_dropped() {
        assert_eq!(without_bom("\u{feff}/score".to_string()), "/score");
        assert_eq!(without_bom("/score".to_string()), "/score");
        assert_eq!(without_bom("fix \u{feff}it".to_string()), "fix \u{feff}it");
    }

    #[test]
    fn every_byte_order_mark_in_front_of_piped_text_is_dropped() {
        // PowerShell 5.1 on a UTF-8 console, with $OutputEncoding set to
        // [Text.Encoding]::UTF8, piped ef bb bf ef bb bf before "/score",
        // which was then graded 15.1 F.
        let piped = without_bom("\u{feff}\u{feff}/score\r\n".to_string());
        assert_eq!(piped, "/score\r\n");
        assert_eq!(yp_core::kind_of(piped.trim()).0, yp_core::Kind::Command);
    }

    #[test]
    fn the_served_page_breaks_a_long_repository_path() {
        // `yp serve` names the repository in the page's subtitle, in a
        // <code> that, at 320 px in English, ran 511 px past the right edge
        // when a folder in the path had a 62-letter name.
        let page = include_str!("../assets/index.html");
        assert!(page.contains("`<code>${repo}</code>"));
        let rule = page
            .split_once(".sub code {")
            .and_then(|(_, rest)| rest.split_once('}'))
            .map(|(rule, _)| rule)
            .expect("a rule for the subtitle's code");
        assert!(rule.contains("overflow-wrap: anywhere"), "{rule}");
    }
}
