//! `yp statusline` -- renders the score Claude Code shows at the bottom of
//! the screen.
//!
//! Status line output is displayed in the terminal and **never enters the
//! model's context**, which is what makes the whole design free of token cost.
//! It is invoked with the session JSON on stdin, debounced at 300ms.
//!
//! Users often already have a status line. `yp install` keeps it, and
//! `--wrapped` runs it first, feeds it the same stdin, and appends our segment
//! to its output, so installing this never costs anyone the status line they
//! had.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use serde::Deserialize;
use yp_core::Kind;

use crate::report::{bar, Style};
use crate::session::{self, Sidecar, UNCHANGED};

#[derive(Debug, Default, Deserialize)]
struct StatusInput {
    #[serde(default)]
    session_id: String,
}

/// Which way the score moved since the previous prompt in this session.
fn trend(sidecar: &Sidecar) -> &'static str {
    match sidecar.previous() {
        Some(previous) if sidecar.latest.total > previous.total + UNCHANGED => "▲",
        Some(previous) if sidecar.latest.total < previous.total - UNCHANGED => "▼",
        Some(_) => "·",
        None => " ",
    }
}

/// What the segment says in place of a number for a prompt it did not grade,
/// or `None` when it should say nothing at all.
fn ungraded(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::Unsupported => Some("not graded · EN/KO only"),
        // A command or follow-up is never recorded by this version, so one is
        // the latest prompt only in a sidecar written before kinds existed.
        // The task before it cannot be told apart in that history either, so
        // there is no honest number to show until the next task.
        _ => None,
    }
}

/// The segment this tool contributes, e.g. `⟦ 860.3 ▓▓▓▓▓▓▓▓▓░ A+ ▲ ⟧`, or
/// `None` when the session has nothing to show.
pub fn segment(sidecar: &Sidecar, style: &Style) -> Option<String> {
    let (kind, _) = sidecar.latest_kind();
    if !kind.is_graded() {
        // Showing the previous task's number here would pass it off as this
        // prompt's, and grading a language the lexicons cannot read would be
        // an F that measures the lexicons. So: why there is no number.
        return ungraded(kind).map(|why| style.dim(&format!("⟦ {why} ⟧")));
    }
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
    Some(out)
}

/// Git Bash, or `None` when it is not installed or this is not Windows.
///
/// On Windows, Claude Code runs status line and hook commands through Git
/// Bash when it is installed, and through PowerShell when it is not. That is
/// the shell a status line was written for, so it is the one that has to run
/// it. It is looked for in `CLAUDE_CODE_GIT_BASH_PATH`, then beside the
/// `git.exe` on `PATH`, then where the Git for Windows installer puts it.
pub fn git_bash() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    // Claude Code's own override for a Git Bash it cannot find.
    if let Some(path) = std::env::var_os("CLAUDE_CODE_GIT_BASH_PATH").map(PathBuf::from) {
        if path.is_file() {
            return Some(path);
        }
    }
    // git.exe on PATH sits in cmd\, bin\ or mingw64\bin\ of the Git root,
    // and Git Bash is bin\bash.exe beside them. A bare bash.exe on PATH is
    // not trusted: C:\Windows\System32\bash.exe is WSL's, which would run
    // the command inside Linux.
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            if !dir.join("git.exe").is_file() {
                continue;
            }
            let roots = [dir.parent(), dir.parent().and_then(Path::parent)];
            for root in roots.into_iter().flatten() {
                let bash = root.join("bin").join("bash.exe");
                if bash.is_file() {
                    return Some(bash);
                }
            }
        }
    }
    // Where the Git for Windows installer puts it, machine-wide or per user.
    for (variable, below) in [
        ("ProgramFiles", "Git"),
        ("ProgramW6432", "Git"),
        ("ProgramFiles(x86)", "Git"),
        ("LOCALAPPDATA", "Programs\\Git"),
    ] {
        if let Some(base) = std::env::var_os(variable) {
            let bash = PathBuf::from(base).join(below).join("bin").join("bash.exe");
            if bash.is_file() {
                return Some(bash);
            }
        }
    }
    None
}

/// PowerShell, found the way Claude Code documents for its PowerShell tool:
/// `pwsh.exe`, PowerShell 7, when it is installed, else `powershell.exe`,
/// the Windows PowerShell 5.1 that every Windows has.
fn powershell() -> PathBuf {
    let on_path = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join("pwsh.exe"))
            .find(|exe| exe.is_file())
    });
    on_path
        .or_else(|| {
            ["ProgramFiles", "ProgramW6432"]
                .into_iter()
                .filter_map(std::env::var_os)
                .map(|base| PathBuf::from(base).join(r"PowerShell\7\pwsh.exe"))
                .find(|exe| exe.is_file())
        })
        .unwrap_or_else(|| PathBuf::from("powershell.exe"))
}

fn powershell_for(exe: &Path, command: &str) -> Command {
    let mut powershell = Command::new(exe);
    powershell.args(["-NoProfile", "-NonInteractive", "-Command", command]);
    powershell
}

/// The process Claude Code would run `command` in: `sh -c` on macOS and
/// Linux; on Windows `bash -c` in Git Bash, or PowerShell without it.
///
/// Never `cmd /C`, which v0.1.1 used: it understands neither `~` nor a
/// shebang, so the docs' own Windows example, `~/.claude/statusline.sh`,
/// printed nothing once wrapped.
fn shell_for(command: &str) -> Command {
    if !cfg!(windows) {
        let mut sh = Command::new("sh");
        sh.args(["-c", command]);
        return sh;
    }
    match git_bash() {
        Some(bash) => {
            let mut bash = Command::new(bash);
            bash.args(["-c", command]);
            bash
        }
        None => powershell_for(&powershell(), command),
    }
}

/// Run the user's own status line command, giving it the same stdin we
/// received, byte for byte, and return whatever it printed.
fn run_wrapped(command: &str, stdin: &[u8]) -> Option<String> {
    run_in(shell_for(command), stdin)
}

fn run_in(mut shell: Command, stdin: &[u8]) -> Option<String> {
    let mut child = shell
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    // Written from a thread while the output is read, so a command that
    // prints before it reads cannot leave both sides waiting on a full pipe.
    let mut pipe = child.stdin.take();
    let input = stdin.to_vec();
    let writer = std::thread::spawn(move || {
        if let Some(pipe) = pipe.as_mut() {
            let _ = pipe.write_all(&input);
        }
    });
    let output = child.wait_with_output().ok();
    let _ = writer.join();
    Some(String::from_utf8_lossy(&output?.stdout).into_owned())
}

/// `wrapped` runs the status line `yp install` kept; `wrap` is one that an
/// earlier `yp install` wrote inline, still honoured until the next install
/// moves it out of the command line.
pub fn run(wrap: Option<String>, wrapped: bool) -> ExitCode {
    // A bug here must not surface as an error. Release builds abort on a
    // panic, which nothing can catch, so the panic hook, the last code that
    // runs, ends the process the way a session with nothing to show ends it:
    // no output and exit 0, rather than a panic message and an abort code.
    std::panic::set_hook(Box::new(|_| std::process::exit(0)));

    let mut raw = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut raw);
    let input: StatusInput = serde_json::from_slice(&raw).unwrap_or_default();

    // Status line output is captured, not attached to a terminal, so
    // auto-detection would always say "no colour". Claude Code renders ANSI
    // here, so force it on unless the user opted out.
    let style = Style::forced();

    let theirs = if wrapped {
        crate::install::wrapped_status_line()
    } else {
        wrap
    };
    let mut line = String::new();
    if let Some(command) = theirs.as_deref().filter(|c| !c.trim().is_empty()) {
        if let Some(existing) = run_wrapped(command, &raw) {
            line.push_str(existing.trim_end_matches(['\n', '\r']));
        }
    }

    if let Some(segment) = session::load(&input.session_id).and_then(|s| segment(&s, &style)) {
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&segment);
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
            kind: Some(Kind::Task),
            language: None,
        }
    }

    fn unsupported(total: f64) -> Entry {
        Entry {
            kind: Some(Kind::Unsupported),
            language: Some("Spanish".into()),
            grade: "F".into(),
            ..entry(total, true)
        }
    }

    fn shown(sidecar: &Sidecar) -> String {
        segment(sidecar, &Style::plain()).expect("a segment")
    }

    #[test]
    fn an_unsupported_prompt_says_why_instead_of_showing_a_number() {
        let mut s = sidecar(742.3, vec![600.0]);
        s.latest = unsupported(149.1);
        let text = shown(&s);
        assert_eq!(text, "⟦ not graded · EN/KO only ⟧");
        assert_eq!(text.lines().count(), 1);
        assert!(!text.contains(|c: char| c.is_ascii_digit()), "got {text}");
        assert!(!text.contains('▓') && !text.contains('░'), "got {text}");
        assert!(!text.ends_with('~'), "got {text}");
    }

    #[test]
    fn an_unsupported_prompt_is_dim_when_coloured() {
        let mut s = sidecar(742.3, vec![]);
        s.latest = unsupported(149.1);
        let text = segment(&s, &Style::forced()).unwrap();
        if std::env::var_os("NO_COLOR").is_none() {
            assert!(text.starts_with("\x1b[2m"), "got {text:?}");
        }
    }

    #[test]
    fn the_trend_skips_an_ungraded_prompt() {
        // 500 after a 700 task is down, whatever the unsupported prompt in
        // between happened to score.
        let mut s = sidecar(500.0, vec![]);
        s.history = vec![unsupported(100.0), entry(700.0, true)];
        assert_eq!(trend(&s), "▼");
        s.history = vec![unsupported(900.0)];
        assert_eq!(trend(&s), " ");
    }

    #[test]
    fn a_slash_command_left_by_v010_shows_nothing_rather_than_its_grade() {
        // v0.1.0 graded "/score" 31.2 F, and after an upgrade the status line
        // kept showing it while `yp explain` said it was not graded.
        let mut s = sidecar(31.2, vec![25.0, 732.9]);
        s.latest.kind = None;
        s.latest_prompt = "/score".into();
        for entry in &mut s.history {
            entry.kind = None;
        }
        assert_eq!(segment(&s, &Style::plain()), None);

        s.latest_prompt = "fix verify_token in src/auth.rs".into();
        let text = shown(&s);
        assert!(text.contains("31.2"), "got {text}");
        // Nothing before it is known to be a task, so there is no trend.
        assert!(!text.contains('▲') && !text.contains('▼'), "got {text}");
    }

    fn sidecar(latest: f64, history: Vec<f64>) -> Sidecar {
        Sidecar {
            version: session::SIDECAR_VERSION,
            session_id: "s".into(),
            updated_unix: 0,
            cwd: String::new(),
            latest: entry(latest, true),
            latest_prompt: "fix it".into(),
            prompt_truncated: false,
            prompt_capped: false,
            history: history.into_iter().map(|t| entry(t, true)).collect(),
        }
    }

    #[test]
    fn segment_shows_the_score_bar_and_grade() {
        let text = shown(&sidecar(742.3, vec![]));
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
        assert_eq!(trend(&sidecar(700.04, vec![700.0])), "·");
        assert_eq!(trend(&sidecar(700.0, vec![])), " ");
    }

    #[test]
    fn a_renormalised_score_is_marked() {
        let mut s = sidecar(742.3, vec![]);
        assert!(shown(&s).ends_with('~'));
        s.latest.renormalized = false;
        assert!(!shown(&s).ends_with('~'));
    }

    #[test]
    fn segment_is_a_single_line() {
        let text = shown(&sidecar(742.3, vec![600.0]));
        assert_eq!(text.lines().count(), 1, "got {text:?}");
    }

    #[test]
    fn wrapping_runs_the_original_command_and_keeps_its_output() {
        // `echo` runs the same in sh, Git Bash and PowerShell.
        let out = run_wrapped("echo mine", b"{}").expect("wrapped command should run");
        assert!(out.contains("mine"), "got {out:?}");
    }

    #[test]
    fn wrapping_a_broken_command_does_not_panic() {
        let out = run_wrapped("this-command-does-not-exist-9f3a", b"{}");
        // Either the shell reports failure and we get empty output, or the
        // spawn fails outright. Both are fine; neither may panic.
        assert!(out.is_none() || out.is_some_and(|o| o.trim().is_empty()));
    }

    /// True when wrapped commands run in a POSIX shell here: always on macOS
    /// and Linux, and on Windows when Git Bash is installed, as it is on
    /// GitHub's Windows runners.
    fn posix_shell() -> bool {
        !cfg!(windows) || git_bash().is_some()
    }

    #[test]
    fn a_wrapped_command_reading_its_stdin_gets_all_of_it() {
        // `input=$(cat)` printed nothing at all when v0.1.1 wrapped it: the
        // outer shell ran the $(cat) and took the status JSON for itself.
        if !posix_shell() {
            return;
        }
        let json = br#"{"session_id":"abc","model":{"display_name":"Opus"}}"#;
        let out = run_wrapped(r#"input=$(cat); echo "mine:${#input}""#, json).unwrap();
        assert_eq!(out.trim(), format!("mine:{}", json.len()));
    }

    #[test]
    fn a_wrapped_command_keeps_its_quotes_and_expansions() {
        // Each of these was printed garbled, or not at all, when the command
        // was quoted into yp's own command line.
        if !posix_shell() {
            return;
        }
        for (command, expected) in [
            (r#"echo "[$(basename "/x/proj")]""#, "[proj]"),
            (r#"v=inner; echo "$v" '$v'"#, "inner $v"),
            ("echo `echo ticks`", "ticks"),
            (r#"printf '%s\n' "a \"quoted\" word""#, r#"a "quoted" word"#),
            (r"printf '%s\n' 'back\slash'", r"back\slash"),
        ] {
            let out = run_wrapped(command, b"{}").unwrap();
            assert_eq!(out.trim(), expected, "{command}");
        }
    }

    #[test]
    fn without_git_bash_a_wrapped_command_gets_our_stdin_in_powershell() {
        // Windows PowerShell 5.1 is on every Windows, Git Bash or not; the
        // pwsh.exe that is preferred when installed takes the same flags.
        if !cfg!(windows) {
            return;
        }
        let json = br#"{"session_id":"abc","model":{"display_name":"Opus"}}"#;
        let script = r#"$text = $input | Out-String; Write-Output ("mine:" + $text.Trim().Length)"#;
        let out = run_in(powershell_for(Path::new("powershell.exe"), script), json).unwrap();
        assert_eq!(out.trim(), format!("mine:{}", json.len()));
    }

    #[test]
    fn git_bash_is_never_wsl() {
        if let Some(bash) = git_bash() {
            let text = bash.to_string_lossy().to_ascii_lowercase();
            assert!(!text.contains("system32"), "got {}", bash.display());
            assert!(!text.contains("windowsapps"), "got {}", bash.display());
        }
    }
}
