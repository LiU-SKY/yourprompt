//! `yp hook` -- the `UserPromptSubmit` hook.
//!
//! # The one rule
//!
//! **This command must never write anything to stdout.**
//!
//! Claude Code takes a `UserPromptSubmit` hook's stdout and injects it into
//! the model's context as plain text. Anything printed here would be tokens
//! spent on every single prompt the user types, forever. The score therefore
//! goes to a sidecar file (see [`crate::session`]) and reaches the user
//! through the status line, whose output never enters the context.
//!
//! # The second rule
//!
//! It must never fail loudly either. Exit code 2 would *block the user's
//! prompt* and erase it; any other non-zero code puts an error notice in their
//! transcript. A scoring tool has no business doing either, so every error
//! path here ends in a silent `ExitCode::SUCCESS`.

use std::io::Read;
use std::process::ExitCode;

use serde::Deserialize;

use crate::session;

/// The subset of the hook payload this command needs.
///
/// Everything is optional: the payload is Claude Code's to evolve, and a
/// missing field must degrade rather than fail.
#[derive(Debug, Default, Deserialize)]
struct HookInput {
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    cwd: String,
}

/// Append a line to a debug log, but only when `YP_DEBUG` is set.
///
/// Deliberately a file rather than stderr: stderr from a hook is surfaced to
/// the user, and this is for us, not them.
fn debug(message: &str) {
    if std::env::var_os("YP_DEBUG").is_none() {
        return;
    }
    let Some(dir) = session::state_dir() else {
        return;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("debug.log"))
    {
        let _ = writeln!(file, "{message}");
    }
}

pub fn run() -> ExitCode {
    // Note what is *absent* from this function: any use of `println!`,
    // `print!`, or `std::io::stdout()`. That is the point.
    let mut raw = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut raw) {
        debug(&format!("could not read stdin: {e}"));
        return ExitCode::SUCCESS;
    }

    let input: HookInput = match serde_json::from_str(&raw) {
        Ok(input) => input,
        Err(e) => {
            debug(&format!("could not parse hook payload: {e}"));
            return ExitCode::SUCCESS;
        }
    };

    if input.prompt.trim().is_empty() {
        debug("empty prompt; nothing to score");
        return ExitCode::SUCCESS;
    }

    // Load the index if one exists; never build it. Indexing a large
    // repository takes seconds, and nothing here may delay a prompt.
    let cwd = if input.cwd.is_empty() {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    } else {
        std::path::PathBuf::from(&input.cwd)
    };
    let corpus = crate::repo::load_for(&cwd);
    if corpus.is_none() {
        debug("no repository index; scoring without the grounding axis");
    }

    let Some(score) = yp_core::score_with(
        &input.prompt,
        corpus.as_ref().map(|c| c as &dyn yp_core::Corpus),
    ) else {
        debug("language resources unavailable");
        return ExitCode::SUCCESS;
    };

    let entry = session::Entry::from_score(&score, &input.prompt);
    debug(&format!(
        "session={} total={} grade={}",
        input.session_id, entry.total, entry.grade
    ));

    if let Err(e) = session::record(&input.session_id, &input.cwd, &input.prompt, entry) {
        debug(&format!("could not write sidecar: {e}"));
    }

    ExitCode::SUCCESS
}
