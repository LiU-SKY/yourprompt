//! `yp statusline` -- renders the score Claude Code shows at the bottom of
//! the screen.
//!
//! Status line output is displayed in the terminal and **never enters the
//! model's context**, which is what makes the whole design free of token cost.
//! It is invoked with the session JSON on stdin, debounced at 300ms.
//!
//! Users often already have a status line. `--wrap` runs theirs first, feeds
//! it the same stdin, and appends our segment to its output, so installing
//! this never costs anyone the status line they had.

use std::io::{Read, Write};
use std::process::{Command, ExitCode, Stdio};

use serde::Deserialize;

use crate::report::{bar, Style};
use crate::session::{self, Sidecar};

#[derive(Debug, Default, Deserialize)]
struct StatusInput {
    #[serde(default)]
    session_id: String,
}

/// Which way the score moved since the previous prompt in this session.
fn trend(sidecar: &Sidecar) -> &'static str {
    match sidecar.previous() {
        Some(previous) if sidecar.latest.total > previous.total + 0.05 => "▲",
        Some(previous) if sidecar.latest.total < previous.total - 0.05 => "▼",
        Some(_) => "·",
        None => " ",
    }
}

/// The segment this tool contributes, e.g. `⟦ 742.3 ▓▓▓▓▓▓▓░░░ A- ▲ ⟧`.
pub fn segment(sidecar: &Sidecar, style: &Style) -> String {
    let ratio = sidecar.latest.total / 1000.0;
    let body = format!(
        "{:.1} {} {} {}",
        sidecar.latest.total,
        bar(ratio),
        sidecar.latest.grade,
        trend(sidecar),
    );
    let mut out = format!(
        "{}{}{}",
        style.dim("⟦ "),
        style.by_ratio(ratio, body.trim_end()),
        style.dim(" ⟧"),
    );
    if sidecar.latest.renormalized {
        // A renormalised score is not comparable to a grounded one, and the
        // user should be able to see that at a glance rather than being
        // quietly misled.
        out.push_str(&style.dim("~"));
    }
    out
}

/// Run the user's pre-existing status line command, giving it the same stdin
/// we received, and return whatever it printed.
fn run_wrapped(command: &str, stdin_text: &str) -> Option<String> {
    let mut child = if cfg!(windows) {
        Command::new("cmd")
            .args(["/C", command])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
    } else {
        Command::new("sh")
            .args(["-c", command])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
    }
    .ok()?;

    if let Some(stdin) = child.stdin.as_mut() {
        let _ = stdin.write_all(stdin_text.as_bytes());
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn run(wrap: Option<String>) -> ExitCode {
    let mut raw = String::new();
    let _ = std::io::stdin().read_to_string(&mut raw);

    let input: StatusInput = serde_json::from_str(&raw).unwrap_or_default();

    // Status line output is captured, not attached to a terminal, so
    // auto-detection would always say "no colour". Claude Code renders ANSI
    // here, so force it on unless the user opted out.
    let style = Style::forced();

    let mut line = String::new();
    if let Some(command) = wrap.as_deref().filter(|c| !c.trim().is_empty()) {
        if let Some(existing) = run_wrapped(command, &raw) {
            line.push_str(existing.trim_end_matches(['\n', '\r']));
        }
    }

    if let Some(sidecar) = session::load(&input.session_id) {
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&segment(&sidecar, &style));
    }

    // Nothing to say and nothing wrapped: print nothing rather than a blank
    // decoration. This is the state before the session's first prompt.
    if line.is_empty() {
        return ExitCode::SUCCESS;
    }

    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Entry;

    fn entry(total: f64, renormalized: bool) -> Entry {
        Entry {
            total,
            grade: "A-".into(),
            renormalized,
            prompt_chars: 40,
        }
    }

    fn sidecar(latest: f64, history: Vec<f64>) -> Sidecar {
        Sidecar {
            version: session::SIDECAR_VERSION,
            session_id: "s".into(),
            updated_unix: 0,
            cwd: String::new(),
            latest: entry(latest, true),
            history: history.into_iter().map(|t| entry(t, true)).collect(),
        }
    }

    #[test]
    fn segment_shows_the_score_bar_and_grade() {
        let text = segment(&sidecar(742.3, vec![]), &Style::plain());
        assert!(text.contains("742.3"), "got {text}");
        assert!(text.contains("A-"), "got {text}");
        assert!(text.starts_with("⟦"), "got {text}");
        assert!(text.contains('▓') || text.contains('░'), "got {text}");
    }

    #[test]
    fn trend_reflects_the_previous_prompt() {
        assert_eq!(trend(&sidecar(700.0, vec![600.0])), "▲");
        assert_eq!(trend(&sidecar(600.0, vec![700.0])), "▼");
        assert_eq!(trend(&sidecar(700.0, vec![700.0])), "·");
        assert_eq!(trend(&sidecar(700.0, vec![])), " ");
    }

    #[test]
    fn a_renormalised_score_is_marked() {
        let mut s = sidecar(742.3, vec![]);
        assert!(segment(&s, &Style::plain()).ends_with('~'));
        s.latest.renormalized = false;
        assert!(!segment(&s, &Style::plain()).ends_with('~'));
    }

    #[test]
    fn segment_is_a_single_line() {
        let text = segment(&sidecar(742.3, vec![600.0]), &Style::plain());
        assert_eq!(text.lines().count(), 1, "got {text:?}");
    }

    #[test]
    fn wrapping_runs_the_original_command_and_keeps_its_output() {
        // `echo` exists as a builtin in both cmd.exe and sh.
        let out = run_wrapped("echo mine", "{}").expect("wrapped command should run");
        assert!(out.contains("mine"), "got {out:?}");
    }

    #[test]
    fn wrapping_a_broken_command_does_not_panic() {
        let out = run_wrapped("this-command-does-not-exist-9f3a", "{}");
        // Either the shell reports failure and we get empty output, or the
        // spawn fails outright. Both are fine; neither may panic.
        assert!(out.is_none() || out.is_some_and(|o| o.trim().is_empty()));
    }
}
