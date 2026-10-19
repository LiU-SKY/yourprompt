//! `yp explain` -- the full breakdown of the prompt that was just scored.
//!
//! The status line has room for a number. This is where the number gets
//! justified: which action verb was found, which vague pronoun cost what,
//! why the length curve landed where it did.
//!
//! Backing the `/score` slash command. A slash command's `!` lines run
//! through Claude Code's Bash tool, or its PowerShell tool on Windows without
//! Git Bash, and both carry the session id in `CLAUDE_CODE_SESSION_ID`: the
//! Bash tool since Claude Code 2.1.132. So with no `--session`, that is the
//! session explained. Without it this falls back to the most recently
//! written sidecar, preferring one whose working directory matches. That
//! guess picked the wrong session when two shared a repository: `/score`
//! typed in one explained the other's "fix the bug".
//!
//! Re-scoring the stored prompt rather than storing the whole breakdown keeps
//! the sidecar small, and is free: scoring is deterministic, so the number
//! here is always the number the status line showed.
//!
//! The hook does not record slash commands, so `/score` itself never becomes
//! the prompt being explained. A prompt in a language the lexicons cannot
//! read is recorded but not graded, and this says so rather than explaining a
//! number nobody should read.

use std::process::ExitCode;

use crate::report::{self, Style};
use crate::session::{self, UNCHANGED};

/// The session Claude Code says this command is running in, if it says.
fn current_session() -> Option<String> {
    std::env::var("CLAUDE_CODE_SESSION_ID")
        .ok()
        .filter(|id| !id.trim().is_empty())
}

pub fn run(session_id: Option<String>, no_color: bool) -> ExitCode {
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();

    let known = session_id.or_else(current_session);
    let sidecar = match known.as_deref() {
        Some(id) => session::load(id),
        None => session::most_recent(&cwd),
    };

    let Some(sidecar) = sidecar else {
        if known.is_some() {
            println!("No scored prompt found yet in this session.");
        } else {
            println!("No scored prompt found yet.");
        }
        println!("The score appears once you send a prompt with the hook installed.");
        println!("Run `yp install` if you have not already.");
        return ExitCode::SUCCESS;
    };

    let (kind, language) = sidecar.latest_kind();
    if let Some(why) = report::not_graded(kind, language.as_deref()) {
        println!("Your last prompt was not graded: {why}.");
        if let Some(previous) = sidecar.previous() {
            println!(
                "The last graded prompt in this session scored {:.1} / 1000 ({}).",
                previous.total, previous.grade
            );
        }
        return ExitCode::SUCCESS;
    }

    if sidecar.latest_prompt.is_empty() {
        // A sidecar written before prompt storage existed, or one whose
        // prompt was empty. Report what is known rather than nothing.
        println!(
            "Last score: {:.1} / 1000 ({})",
            sidecar.latest.total, sidecar.latest.grade
        );
        println!("The prompt itself was not stored, so there is no breakdown to show.");
        return ExitCode::SUCCESS;
    }

    // Ground against the repository the prompt was written in, not whichever
    // directory the slash command happened to run from.
    let root = if sidecar.cwd.is_empty() {
        std::path::PathBuf::from(&cwd)
    } else {
        std::path::PathBuf::from(&sidecar.cwd)
    };
    let corpus = crate::repo::load_for(&root);

    let Some(score) = yp_core::score_with(
        &sidecar.latest_prompt,
        corpus.as_ref().map(|c| c as &dyn yp_core::Corpus),
    ) else {
        eprintln!("yp: bundled language resources failed to load");
        return ExitCode::FAILURE;
    };

    let style = if no_color {
        Style::plain()
    } else {
        Style::detect()
    };

    if sidecar.prompt_truncated {
        println!(
            "note: the prompt was longer than the sidecar keeps, so this \
             breakdown is of the first part only.\n      the status line \
             score of {:.1} was computed on the whole prompt.",
            sidecar.latest.total
        );
    }

    print!("{}", report::full(&score, &style));

    // A sidecar written before kinds existed may hold a "continue" or a
    // "/score" as its latest task. Its report already says it is not graded,
    // and a move from the previous score would be a move nobody made.
    if !score.kind.is_graded() {
        return ExitCode::SUCCESS;
    }

    if let Some(previous) = sidecar.previous() {
        let delta = sidecar.latest.total - previous.total;
        if delta.abs() <= UNCHANGED {
            println!(
                "  same as your previous prompt in this session ({:.1})\n",
                previous.total
            );
        } else {
            let direction = if delta > 0.0 { "up" } else { "down" };
            println!(
                "  {} {:.1} from your previous prompt in this session ({:.1})\n",
                direction,
                delta.abs(),
                previous.total
            );
        }
    }

    ExitCode::SUCCESS
}
