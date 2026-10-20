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
//!
//! # What is recorded
//!
//! Only what the status line should report. A slash command and a follow-up
//! such as "continue" are skipped entirely: in a live session "continue",
//! "yes" and "ok do it" each replaced the last task's score with an F, and
//! `/score` -- which reaches this hook before Claude Code expands it -- was
//! recorded and then explained in place of the prompt the user asked about.
//! A prompt in a language the lexicons cannot read is recorded with its kind,
//! so the status line can say it was not graded.

use std::io::Read;
use std::process::ExitCode;

use serde::Deserialize;
use yp_core::Kind;

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
    // The release profile aborts on panic, so no panic can be caught, but the
    // panic hook still runs before the abort. Exiting from it keeps the second
    // rule. One Korean file name in the repository used to be enough to panic
    // the index lookup, and the hook then aborted with the message on stderr:
    // exit code 0xC0000409 on Windows.
    std::panic::set_hook(Box::new(|_| std::process::exit(0)));

    // Note what is *absent* from this function: any use of `println!`,
    // `print!`, or `std::io::stdout()`. That is the point.
    let mut raw = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut raw) {
        debug(&format!("could not read stdin: {e}"));
        return ExitCode::SUCCESS;
    }

    let input: HookInput = match parse(&raw) {
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

    // Classified before the index is loaded, so a "yes" costs no file read.
    // The sidecar keeps the last task, which is what the status line and
    // `/score` should still be showing.
    let (kind, _) = yp_core::kind_of(&input.prompt);
    if matches!(kind, Kind::Command | Kind::FollowUp) {
        debug(&format!(
            "session={} kind={kind:?}; not recorded",
            input.session_id
        ));
        return ExitCode::SUCCESS;
    }

    // Load the index if one exists; never build it. Indexing a large
    // repository takes seconds, and nothing here may delay a prompt. Nor
    // may finding out where the index is, which reads the working directory:
    // past the deadline the prompt is scored without one.
    let cwd = if input.cwd.is_empty() {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    } else {
        std::path::PathBuf::from(&input.cwd)
    };
    let deadline = crate::repo::LOOKUP_DEADLINE;
    let started = std::time::Instant::now();
    let corpus = match crate::repo::load_for_within(&cwd, deadline) {
        Some(found) => {
            debug(&format!(
                "looked for the repository index in {} ms",
                started.elapsed().as_millis()
            ));
            found
        }
        None => {
            debug(&format!(
                "gave up looking for the repository index after {} ms",
                deadline.as_millis()
            ));
            None
        }
    };
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
    debug(&scored_line(&input.session_id, score.kind, &entry));

    if let Err(e) = session::record(&input.session_id, &input.cwd, &input.prompt, entry) {
        debug(&format!("could not write sidecar: {e}"));
    }

    ExitCode::SUCCESS
}

/// The debug line for a prompt that was scored and recorded.
///
/// The kind is written as the line for a skipped prompt writes it,
/// `kind=Task`. The entry's own field is an `Option`, for sidecars written
/// before kinds existed, and logging it printed `kind=Some(Task)` beside
/// `kind=FollowUp`.
fn scored_line(session_id: &str, kind: Kind, entry: &session::Entry) -> String {
    format!(
        "session={session_id} kind={kind:?} total={} grade={}",
        entry.total, entry.grade
    )
}

/// Parse the payload, with any lone surrogate in it read as U+FFFD.
///
/// Node's `JSON.stringify` writes half of a surrogate pair, such as an emoji
/// cut in two, as an escape like `\ud800`, and serde_json refuses that as a
/// string. The hook used to stop there without recording anything, and the
/// status line went on showing the previous prompt's grade as this one's.
fn parse(raw: &str) -> serde_json::Result<HookInput> {
    serde_json::from_str(raw).or_else(|e| match replace_lone_surrogates(raw) {
        Some(repaired) => serde_json::from_str(&repaired),
        None => Err(e),
    })
}

/// `json` with the escape of every unpaired UTF-16 surrogate replaced by the
/// escape of U+FFFD, or `None` when it has none.
fn replace_lone_surrogates(json: &str) -> Option<String> {
    const LEADING: std::ops::RangeInclusive<u16> = 0xD800..=0xDBFF;
    const TRAILING: std::ops::RangeInclusive<u16> = 0xDC00..=0xDFFF;

    let bytes = json.as_bytes();
    let mut out = String::with_capacity(json.len());
    // How much of `json` has been copied to `out`.
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            i += 1;
            continue;
        }
        let Some(unit) = unicode_escape(bytes, i) else {
            // Skip the escaped character too, so that the second backslash
            // of `\\ud800` is never read as the start of an escape.
            i += 2;
            continue;
        };
        let paired = LEADING.contains(&unit)
            && unicode_escape(bytes, i + 6).is_some_and(|next| TRAILING.contains(&next));
        if paired {
            i += 12;
        } else if LEADING.contains(&unit) || TRAILING.contains(&unit) {
            out.push_str(&json[copied..i]);
            out.push_str("\\uFFFD");
            i += 6;
            copied = i;
        } else {
            i += 6;
        }
    }
    if copied == 0 {
        return None;
    }
    out.push_str(&json[copied..]);
    Some(out)
}

/// The code unit of the `\uXXXX` escape at `at` in a JSON text, if one is
/// there.
fn unicode_escape(bytes: &[u8], at: usize) -> Option<u16> {
    match bytes.get(at..at + 6)? {
        [b'\\', b'u', hex @ ..] if hex.iter().all(u8::is_ascii_hexdigit) => {
            u16::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the JSON string literal `json` reads as once repaired.
    fn read(json: &str) -> String {
        let repaired = replace_lone_surrogates(json).unwrap_or_else(|| json.to_string());
        serde_json::from_str(&repaired).unwrap()
    }

    #[test]
    fn a_lone_surrogate_is_read_as_a_replacement_character() {
        let input = parse("{\"prompt\": \"fix \\ud800 it\", \"session_id\": \"s1\"}").unwrap();
        assert_eq!(input.prompt, "fix \u{FFFD} it");
        assert_eq!(input.session_id, "s1");
        assert_eq!(read("\"fix it \\uDC00\""), "fix it \u{FFFD}");
        assert_eq!(read("\"\\ud800\""), "\u{FFFD}");
    }

    #[test]
    fn a_surrogate_pair_is_left_alone() {
        assert_eq!(replace_lone_surrogates("\"\\ud83d\\ude00\""), None);
        assert_eq!(read("\"\\ud83d\\ude00\""), "\u{1F600}");
        assert_eq!(read("\"\\ud83d\\ude00\\ude00\""), "\u{1F600}\u{FFFD}");
        assert_eq!(read("\"\\udbff\\udbff\\udfff\""), "\u{FFFD}\u{10FFFF}");
    }

    #[test]
    fn a_leading_surrogate_needs_a_trailing_one_right_after_it() {
        assert_eq!(read("\"\\ud800\\u0041\""), "\u{FFFD}A");
        assert_eq!(read("\"\\ud800\\n\""), "\u{FFFD}\n");
        assert_eq!(read("\"\\ud800 \\udc00\""), "\u{FFFD} \u{FFFD}");
    }

    #[test]
    fn an_escaped_backslash_does_not_start_an_escape() {
        // A backslash and the letters "ud800", not an escape.
        assert_eq!(replace_lone_surrogates("\"\\\\ud800\""), None);
        assert_eq!(read("\"\\\\ud800\""), "\\ud800");
        assert_eq!(read("\"\\\\\\ud800\""), "\\\u{FFFD}");
    }

    #[test]
    fn a_scored_prompt_is_logged_with_its_kind_as_a_skipped_one_is() {
        let score = yp_core::score("fix the parser in src/cli.rs").unwrap();
        let entry = session::Entry::from_score(&score, "fix the parser in src/cli.rs");
        let line = scored_line("dbg1", score.kind, &entry);
        assert!(line.starts_with("session=dbg1 kind=Task total="), "{line}");
        assert!(!line.contains("Some("), "{line}");
        // The line for a skipped prompt, which reads `kind={kind:?}`.
        assert_eq!(format!("kind={:?}", Kind::FollowUp), "kind=FollowUp");
    }

    #[test]
    fn a_payload_broken_in_another_way_is_still_refused() {
        assert!(parse("{\"prompt\": \"fix it\"").is_err());
        assert!(parse("{\"prompt\": \"fix \\ud800 it\"").is_err());
        assert!(parse("{\"prompt\": \"\\ud800 \\x\"}").is_err());
    }
}
