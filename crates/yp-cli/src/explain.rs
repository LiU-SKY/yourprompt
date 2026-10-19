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
//! here is the number the status line showed, as long as what it was scored
//! against has not changed. When the repository index has been rebuilt
//! since, it can differ, and this says so: after three files were added to
//! the demo's sample repository and it was indexed again, the status line
//! went on showing 860.3 (A+) while the breakdown came to 842.0 (A).
//!
//! The hook does not record slash commands, so `/score` itself never becomes
//! the prompt being explained. A prompt in a language the lexicons cannot
//! read is recorded but not graded, and this says so rather than explaining a
//! number nobody should read.

use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use yp_core::Score;

use crate::repo::{self, IndexCorpus};
use crate::report::{self, Style};
use crate::session::{self, Sidecar, UNCHANGED};

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
    // directory the slash command happened to run from, and find its index
    // the way the hook did, so a lookup the hook gave up on is given up on
    // here too.
    let root = if sidecar.cwd.is_empty() {
        std::path::PathBuf::from(&cwd)
    } else {
        std::path::PathBuf::from(&sidecar.cwd)
    };
    let lookup = repo::load_for_within(&root, repo::LOOKUP_DEADLINE);
    let gave_up = lookup.is_none();
    let corpus = lookup.flatten();

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
        let scored = if sidecar.prompt_capped {
            format!(
                "the first {} KB of the prompt, which is as much as yp reads",
                yp_core::prompt::MAX_INSTRUCTION_BYTES / 1024
            )
        } else {
            "the whole prompt".to_string()
        };
        println!(
            "note: the prompt was longer than the sidecar keeps, so this \
             breakdown is of the first part only.\n      the status line \
             score of {:.1} was computed on {scored}.",
            sidecar.latest.total
        );
    }
    if gave_up {
        println!(
            "note: finding the index for {} took longer than {} ms, so this \
             breakdown is scored without one.",
            root.display(),
            repo::LOOKUP_DEADLINE.as_millis()
        );
    } else if !sidecar.prompt_truncated {
        let written = corpus.as_ref().and_then(IndexCorpus::written);
        if let Some(why) = moved_since(&sidecar, &score, written) {
            println!("{why}");
        }
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

/// A note for when the prompt, scored again, does not come to what the
/// status line showed for it, or `None` when it does.
///
/// Both were worked out the same way from the same text, so what differs is
/// what they were worked out against: an index built, rebuilt or deleted
/// since, which `index_written` tells apart, or else a different build of yp.
fn moved_since(
    sidecar: &Sidecar,
    score: &Score,
    index_written: Option<SystemTime>,
) -> Option<String> {
    let shown = &sidecar.latest;
    // Compared as both are shown, to one decimal.
    if format!("{:.1}", shown.total) == format!("{:.1}", score.display_total()) {
        return None;
    }
    let scored_at = UNIX_EPOCH + Duration::from_secs(sidecar.updated_unix);
    let index_is_newer = index_written.is_none_or(|written| written > scored_at);
    let how = match (shown.renormalized, score.renormalized) {
        // Built since, or not found in time by the hook.
        (true, false) => {
            "without an index of this repository.\n      this breakdown uses the one there is now."
        }
        (false, true) => {
            "against an index of this repository that is gone.\n      this breakdown is scored without one."
        }
        (false, false) if index_is_newer => {
            "against an older index of this repository.\n      this breakdown uses the current one."
        }
        _ => "by another build of yp.\n      this breakdown is this build's.",
    };
    Some(format!(
        "note: the status line showed {:.1} ({}) for this prompt, scored {how}",
        shown.total, shown.grade
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Entry;

    /// A sidecar whose latest prompt the status line showed as `total`.
    fn shown(total: f64, grade: &str, renormalized: bool) -> Sidecar {
        Sidecar {
            version: session::SIDECAR_VERSION,
            session_id: "s".into(),
            updated_unix: 1_000_000,
            cwd: String::new(),
            latest: Entry {
                total,
                grade: grade.into(),
                renormalized,
                prompt_chars: 40,
                kind: Some(yp_core::Kind::Task),
                language: None,
            },
            latest_prompt: "fix verify_token in src/auth/login.rs".into(),
            prompt_truncated: false,
            prompt_capped: false,
            history: Vec::new(),
        }
    }

    /// The same prompt scored now, coming to whatever `yp_core` makes of it.
    fn now(renormalized: bool) -> Score {
        let mut score = yp_core::score("fix verify_token in src/auth/login.rs").unwrap();
        score.renormalized = renormalized;
        score
    }

    fn at(unix: u64) -> Option<SystemTime> {
        Some(UNIX_EPOCH + Duration::from_secs(unix))
    }

    #[test]
    fn the_same_number_needs_no_note() {
        let score = now(false);
        let sidecar = shown(score.display_total(), score.grade, false);
        assert_eq!(moved_since(&sidecar, &score, at(2_000_000)), None);
    }

    #[test]
    fn a_number_from_an_older_index_says_so() {
        // The reported case: 860.3 (A+) in the status line, 842.0 (A) from
        // /score once the index had been rebuilt with three more files.
        let note = moved_since(&shown(860.3, "A+", false), &now(false), at(1_000_060)).unwrap();
        assert!(note.contains("showed 860.3 (A+)"), "{note}");
        assert!(note.contains("an older index"), "{note}");
        assert_eq!(note.lines().count(), 2, "{note}");
    }

    #[test]
    fn an_index_built_or_lost_since_is_named_as_the_reason() {
        let built = moved_since(&shown(799.8, "A-", true), &now(false), at(1_000_060)).unwrap();
        assert!(
            built.contains("without an index of this repository"),
            "{built}"
        );
        let lost = moved_since(&shown(860.3, "A+", false), &now(true), None).unwrap();
        assert!(
            lost.contains("an index of this repository that is gone"),
            "{lost}"
        );
    }

    #[test]
    fn with_the_index_unchanged_the_difference_is_yp_itself() {
        let older_index = at(999_000);
        for (renormalized, written) in [(false, older_index), (true, None)] {
            let note = moved_since(
                &shown(123.4, "F", renormalized),
                &now(renormalized),
                written,
            )
            .unwrap();
            assert!(note.contains("another build of yp"), "{note}");
        }
    }
}
