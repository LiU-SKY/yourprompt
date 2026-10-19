//! `yp install` -- register the hooks, the status line and `/score` with
//! Claude Code.
//!
//! Claude Code plugins can ship hooks but *cannot* ship a `statusLine`: a
//! plugin's settings.json only honours `agent` and `subagentStatusLine`. So
//! the status line has to be written into the user's own settings, which
//! means touching a file they own and probably care about.
//!
//! Two rules follow from that. Never clobber a status line the user already
//! has -- wrap it instead, so their existing one keeps working with our
//! segment appended. And always leave a backup and a way out (`--uninstall`,
//! `--print-only`).
//!
//! # How the commands are written
//!
//! Claude Code hands a command string to `sh -c` on macOS and Linux, and on
//! Windows to Git Bash, or to PowerShell when Git Bash is not installed. Git
//! Bash reads a backslash as an escape: v0.1.1 wrote `C:\Users\...\yp.exe
//! hook`, Git Bash looked for `C:Users...yp.exe`, and every command it wrote
//! exited 127, so no prompt was ever scored.
//!
//! So the hooks skip the shell. They are written in the exec form, the
//! executable in `command` and its arguments in `args`, which Claude Code
//! spawns directly. No shell can misread a path it never sees, a space in one
//! needs no quoting, and no prompt waits for a shell to start: on Windows the
//! hook took a median 11 ms spawned directly, 51 ms through Git Bash and
//! 225 ms through PowerShell. Claude Code reads `args` since 2.1.139. An
//! older one that skips them runs the bare executable, and `main` keeps that
//! from blocking the prompt.
//!
//! The status line has no exec form, so it stays a command string. It names
//! the executable with forward slashes, which Git Bash, PowerShell and `sh`
//! all read as written, and quotes it only when it has to, the way the shell
//! found at install time quotes. No one string serves both Windows shells
//! when quotes are needed: PowerShell runs a quoted path only behind its `&`
//! operator, and Git Bash rejects the `&`.
//!
//! # What is kept outside settings.json
//!
//! A status line that gets wrapped is kept verbatim, every field of it, in
//! `install.json` in the state directory, and `yp statusline --wrapped` runs
//! it from there. Quoting it into our own command line, as v0.1.1 did, handed
//! it to the outer shell first: `$(...)`, `$VAR` and backticks were expanded
//! before yp started, embedded quotes came out garbled, and `input=$(cat)`
//! swallowed the status JSON meant for both of us, so the line went blank.
//! `install.json` also records the `/score` command file as written, so a
//! later run can tell it from one the user wrote or edited.
//!
//! The transformations themselves are pure functions over `serde_json::Value`
//! and [`Saved`], so every case below is tested without going near a real
//! settings file.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::session;

/// What changed, in words, for the user to read.
pub type Notes = Vec<String>;

/// The `/score` command: the same file the plugin ships, so there is one.
pub const SCORE_COMMAND: &str = include_str!("../../../plugin/commands/score.md");

/// The arguments of the session-start index build. `--max-age` makes an
/// unconditional session-start hook cheap: a session opened minutes after
/// the last one reuses the existing index.
const INDEX_ARGS: [&str; 4] = ["index", "--quiet", "--max-age", "900"];

/// Seconds Claude Code gives each hook, as the plugin's hooks.json sets them.
/// It cancels a hook that runs out of time and goes on without it, so
/// scoring can never hold a prompt longer than this. Its own default for a
/// `UserPromptSubmit` hook is 30 seconds.
const HOOK_TIMEOUT: u64 = 5;
const INDEX_TIMEOUT: u64 = 60;

/// The shell that will read the status line command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    /// `sh` on macOS and Linux, Git Bash on Windows.
    Posix,
    /// Windows without Git Bash.
    PowerShell,
}

impl Shell {
    /// The shell Claude Code will use on this machine.
    fn here() -> Self {
        if cfg!(windows) && crate::statusline::git_bash().is_none() {
            Shell::PowerShell
        } else {
            Shell::Posix
        }
    }
}

/// What `yp install` keeps outside settings.json, in the state directory.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    /// The status line found when installing, exactly as it was, for
    /// `yp statusline --wrapped` to run and `--uninstall` to put back.
    #[serde(
        default,
        rename = "statusLine",
        skip_serializing_if = "Option::is_none"
    )]
    pub status_line: Option<Value>,
    /// The `/score` command file as `yp install` last wrote it.
    #[serde(
        default,
        rename = "scoreCommand",
        skip_serializing_if = "Option::is_none"
    )]
    pub score_command: Option<String>,
}

/// Split off the executable a command line starts with, unquoted, and return
/// it with the rest of the line.
///
/// Honours single and double quotes, a backslash before a quote or a space,
/// and PowerShell's `&` call operator, which is how a quoted path has to be
/// written for PowerShell. Any other backslash is kept: it is a separator in
/// the Windows paths v0.1.1 wrote, not an escape.
fn split_exe(command: &str) -> (String, &str) {
    let mut text = command.trim_start();
    let powershell = text.starts_with('&');
    if powershell {
        text = text[1..].trim_start();
    }
    let mut exe = String::new();
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        match c {
            c if c.is_whitespace() => return (exe, &text[at..]),
            '\'' => {
                while let Some((_, c)) = chars.next() {
                    if c != '\'' {
                        exe.push(c);
                    } else if powershell && chars.peek().is_some_and(|&(_, next)| next == '\'') {
                        // PowerShell writes a quote inside quotes as two.
                        chars.next();
                        exe.push('\'');
                    } else {
                        break;
                    }
                }
            }
            '"' => {
                while let Some((_, c)) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' if !powershell
                            && chars.peek().is_some_and(|&(_, next)| {
                                matches!(next, '"' | '\\' | '$' | '`')
                            }) =>
                        {
                            exe.push(chars.next().map_or('\\', |(_, next)| next));
                        }
                        c => exe.push(c),
                    }
                }
            }
            '\\' if !powershell
                && chars
                    .peek()
                    .is_some_and(|&(_, next)| matches!(next, ' ' | '\'' | '"')) =>
            {
                exe.push(chars.next().map_or('\\', |(_, next)| next));
            }
            c => exe.push(c),
        }
    }
    (exe, "")
}

/// True when this executable path names yp.
///
/// Split on both separators by hand rather than using `Path::file_stem`. On
/// Unix, `Path` does not treat a backslash as a separator, so a settings.json
/// written on Windows and read from WSL would not be recognised as ours --
/// and `yp install` would then wrap its own command instead of updating it.
/// Sharing a home directory between Windows and WSL is common enough to be
/// worth handling.
fn is_yp(exe: &str) -> bool {
    let file = exe.rsplit(['/', '\\']).next().unwrap_or(exe);
    let stem = file.split('.').next().unwrap_or(file);
    stem.eq_ignore_ascii_case("yp")
}

/// A name with no directory in it, which the shell finds on `PATH`.
fn is_bare(exe: &str) -> bool {
    !exe.contains(['/', '\\'])
}

/// The executable, and what follows `subcommand`, when this command line
/// runs yp's `subcommand`. An unrelated command that merely mentions it, such
/// as `python statusline.py`, is not ours.
fn ours<'a>(command: &'a str, subcommand: &str) -> Option<(String, &'a str)> {
    let (exe, rest) = split_exe(command);
    let after = rest.trim_start().strip_prefix(subcommand)?;
    let whole_word = after.is_empty() || after.starts_with(char::is_whitespace);
    (whole_word && is_yp(&exe)).then_some((exe, after))
}

/// True for a drive-letter or UNC path.
fn is_windows_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    drive || path.starts_with(r"\\")
}

/// The path as a command string has to name it: a Windows path with forward
/// slashes, which Git Bash and PowerShell both accept, and without the `\\?\`
/// prefix neither shell understands.
fn portable(exe: &str) -> String {
    let plain = if let Some(unc) = exe.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        exe.strip_prefix(r"\\?\").unwrap_or(exe).to_string()
    };
    if is_windows_path(&plain) {
        plain.replace('\\', "/")
    } else {
        plain
    }
}

/// Two spellings of one executable. Windows paths compare without regard to
/// case or separator, as Windows itself does.
fn same_exe(a: &str, b: &str) -> bool {
    let (a, b) = (portable(a), portable(b));
    if is_windows_path(&a) {
        a.eq_ignore_ascii_case(&b)
    } else {
        a == b
    }
}

/// The executable as the first word of a command line for `shell`, quoted
/// only when it holds something a shell would split, expand or misread:
/// `C:/Users/John Smith/...` is three words to any shell unquoted.
fn exe_word(exe: &str, shell: Shell) -> String {
    let exe = portable(exe);
    let plain = !exe.is_empty()
        && exe
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | ':'));
    if plain {
        return exe;
    }
    match shell {
        // Inside single quotes nothing is special but the quote itself.
        Shell::Posix => format!("'{}'", exe.replace('\'', r"'\''")),
        // PowerShell treats a quoted path as a string unless the call
        // operator runs it; Git Bash would reject the `&`, which is why this
        // form is written only when Git Bash is not installed.
        Shell::PowerShell => format!("& '{}'", exe.replace('\'', "''")),
    }
}

fn statusline_command(exe: &str, shell: Shell, wrapped: bool) -> String {
    let exe = exe_word(exe, shell);
    if wrapped {
        format!("{exe} statusline --wrapped")
    } else {
        format!("{exe} statusline")
    }
}

/// How v0.1.1 quoted the status line it wrapped into `--wrap "..."`.
fn quote(command: &str) -> String {
    format!("\"{}\"", command.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The text of a double-quoted string, honouring the backslash escapes
/// [`quote`] writes, so a wrapped command containing quotes round-trips.
fn unquote(quoted: &str) -> Option<String> {
    let mut chars = quoted.strip_prefix('"')?.chars();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push(chars.next()?),
            '"' => return Some(out),
            other => out.push(other),
        }
    }
    None
}

/// The status line a v0.1.1 install wrapped, when `rest`, what follows
/// `statusline`, is exactly the `--wrap "..."` that version wrote.
///
/// Only that form moves to [`Saved`]. Anything else after `statusline` was
/// written by hand, such as `--wrap 'my-status.sh'`, and stays as it is:
/// rewriting it as a plain `statusline` would lose the command it names.
fn legacy_wrap(rest: &str) -> Option<String> {
    let quoted = rest.trim().strip_prefix("--wrap ")?;
    let original = unquote(quoted)?;
    (quote(&original) == quoted).then_some(original)
}

/// True when what follows `statusline` runs the status line kept in [`Saved`].
fn runs_kept(rest: &str) -> bool {
    rest.split_whitespace().any(|word| word == "--wrapped")
}

/// Why a command of ours that named `found` now runs `exe` the way `how`
/// says, or `None` when nothing is worth saying beyond `how`.
fn rewrite_note(what: &str, found: &str, exe: &str, how: &str) -> Option<String> {
    let now = portable(exe);
    if !same_exe(found, exe) {
        // Whether a path from another platform exists cannot be told from
        // here: a settings file shared with WSL names Windows paths.
        let native = is_windows_path(found) == cfg!(windows);
        let gone = if native && !Path::new(found).exists() {
            ", which no longer exists"
        } else {
            ""
        };
        return Some(format!("the {what} ran {found}{gone}; it now runs {now}"));
    }
    found.contains('\\').then(|| {
        format!(
            "rewrote the {what} {how}: Git Bash, which Claude Code uses on \
             Windows, read the backslashes in its path as escapes and never ran it"
        )
    })
}

/// The status line part of [`apply`].
fn apply_status_line(
    settings: &mut Value,
    exe: &str,
    shell: Shell,
    saved: &mut Saved,
    notes: &mut Notes,
) {
    let Some(line) = settings.get("statusLine").cloned() else {
        settings["statusLine"] = json!({
            "type": "command",
            "command": statusline_command(exe, shell, false),
        });
        notes.push("registered the status line".into());
        return;
    };
    let command = line
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    if let Some((found, rest)) = ours(&command, "statusline") {
        let noted = notes.len();
        let tail = match legacy_wrap(rest) {
            Some(original) => {
                let mut kept = line.clone();
                kept["command"] = Value::String(original.clone());
                saved.status_line = Some(kept);
                notes.push(format!(
                    "moved the status line it wraps out of its command line, where \
                     the shell rewrote it: {original}"
                ));
                " --wrapped".to_string()
            }
            None => rest.to_string(),
        };
        // A bare `yp` follows PATH, which is the user's choice to make.
        let target = if is_bare(&found) { found.as_str() } else { exe };
        let desired = format!("{} statusline{tail}", exe_word(target, shell));
        if command == desired {
            notes.push("status line already registered; left as is".into());
            return;
        }
        if !is_bare(&found) {
            notes.extend(rewrite_note(
                "status line",
                &found,
                exe,
                "with forward slashes",
            ));
        }
        if notes.len() == noted {
            notes.push("rewrote the status line for the shell Claude Code uses here".into());
        }
        settings["statusLine"]["command"] = Value::String(desired);
        return;
    }

    // Someone else's: keep it whole, and wrap it if there is anything to run.
    saved.status_line = Some(line);
    if command.trim().is_empty() {
        settings["statusLine"] = json!({
            "type": "command",
            "command": statusline_command(exe, shell, false),
        });
        notes.push("replaced a status line that had no command; --uninstall puts it back".into());
    } else {
        settings["statusLine"]["command"] = Value::String(statusline_command(exe, shell, true));
        notes.push(format!(
            "wrapped your existing status line so it keeps working: {command}"
        ));
    }
}

/// Add our hooks and status line to `settings`, returning what changed.
///
/// `saved` receives whatever has to be kept outside the settings file: the
/// status line being wrapped.
pub fn apply(settings: &mut Value, exe: &str, shell: Shell, saved: &mut Saved) -> Notes {
    let mut notes = Notes::new();
    if !settings.is_object() {
        *settings = json!({});
        notes.push("settings file was not a JSON object; started from an empty one".into());
    }

    apply_status_line(settings, exe, shell, saved, &mut notes);
    if exe_word(exe, shell).starts_with('&') {
        notes.push(
            "Git Bash is not installed, so the status line is written the way PowerShell \
             runs a quoted path, which Git Bash cannot: run `yp install` again if you \
             install Git for Windows"
                .into(),
        );
    }

    // Scoring on every prompt, and building the repository index once per
    // session. The index build is deliberately *not* on the prompt path:
    // indexing takes seconds and nothing may delay a prompt.
    let target = Target { exe, shell };
    register(
        settings,
        &mut notes,
        &target,
        "UserPromptSubmit",
        &["hook"],
        HOOK_TIMEOUT,
    );
    register(
        settings,
        &mut notes,
        &target,
        "SessionStart",
        &INDEX_ARGS,
        INDEX_TIMEOUT,
    );

    if plugin_enabled(settings) {
        notes.push(
            "the yourprompt plugin is enabled too, and it registers the same hooks, so \
             every prompt would be scored twice: disable the plugin, or undo this with \
             `yp install --uninstall`"
                .into(),
        );
    }

    notes
}

/// What a command of ours should run: this executable, and, for one that
/// goes through a shell, the shell that will read it.
struct Target<'a> {
    exe: &'a str,
    shell: Shell,
}

/// A new hook handler running yp with `args`, in the exec form.
fn hook_handler(exe: &str, args: &[String]) -> Value {
    json!({ "type": "command", "command": portable(exe), "args": args })
}

/// `handler` moved to the exec form, running `exe` with `args`, with every
/// other field it had kept in the order it had them.
fn exec_form(handler: &Value, exe: &str, args: &[String]) -> Value {
    let mut out = Map::new();
    out.insert("type".into(), json!("command"));
    out.insert("command".into(), json!(portable(exe)));
    out.insert("args".into(), json!(args));
    for (key, value) in handler.as_object().into_iter().flatten() {
        if !matches!(key.as_str(), "type" | "command" | "args") {
            out.insert(key.clone(), value.clone());
        }
    }
    Value::Object(out)
}

/// One of our hook handlers: the executable it names, and how it runs it.
struct Handler {
    exe: String,
    /// The arguments, when they can be passed without a shell: every one
    /// in the exec form, or a shell form made of plain words.
    args: Option<Vec<String>>,
    /// In the shell form, what follows the subcommand. `None` in the exec
    /// form, which has no shell to read anything else.
    rest: Option<String>,
}

/// The hook handler running yp's `subcommand`, in either form.
fn our_handler(handler: &Value, subcommand: &str) -> Option<Handler> {
    let command = handler.get("command")?.as_str()?;
    match handler.get("args").and_then(Value::as_array) {
        Some(args) => {
            let exe = command.trim().to_string();
            let args: Vec<String> = args
                .iter()
                .map(|a| a.as_str().map(str::to_string))
                .collect::<Option<_>>()?;
            let runs = args.first().map(String::as_str) == Some(subcommand);
            (runs && is_yp(&exe)).then_some(Handler {
                exe,
                args: Some(args),
                rest: None,
            })
        }
        None => {
            let (exe, rest) = ours(command, subcommand)?;
            let words: Vec<&str> = rest.split_whitespace().collect();
            let plain = words.iter().all(|w| {
                w.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '='))
            });
            let args = plain.then(|| {
                std::iter::once(subcommand)
                    .chain(words)
                    .map(str::to_string)
                    .collect()
            });
            Some(Handler {
                exe,
                args,
                rest: Some(rest.to_string()),
            })
        }
    }
}

fn group_has(group: &Value, subcommand: &str) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|inner| inner.iter().any(|h| our_handler(h, subcommand).is_some()))
}

/// Bring one handler of ours up to date, saying how when it changed.
///
/// Only what yp owns changes: the executable, and the form. A timeout or any
/// other field the user set stays, and so do arguments they changed.
///
/// A shell command that already runs this executable, written the way the
/// shell here reads it, is left alone: it works, and it works even on a
/// Claude Code too old for the exec form. That is what v0.1.1 wrote on macOS
/// and Linux. Any other one made of plain words, such as the backslash paths
/// v0.1.1 wrote on Windows, moves to the exec form.
fn update_handler(
    handler: &mut Value,
    found: Handler,
    target: &Target,
    subcommand: &str,
    what: &str,
) -> Option<String> {
    // A bare `yp` follows PATH, which is the user's choice to make, and it
    // needs no quoting in any shell.
    if is_bare(&found.exe) {
        return None;
    }
    let as_written = handler["command"].as_str().unwrap_or_default().trim();
    let how = match (found.rest, found.args) {
        // The exec form: only the executable can be out of date.
        (None, _) => {
            if same_exe(&found.exe, target.exe) {
                return None;
            }
            handler["command"] = Value::String(portable(target.exe));
            "to run yp without a shell"
        }
        (Some(rest), Some(args)) => {
            let working = format!("{} {subcommand}{rest}", exe_word(target.exe, target.shell));
            if same_exe(&found.exe, target.exe) && as_written == working.trim() {
                return None;
            }
            *handler = exec_form(handler, target.exe, &args);
            "to run yp without a shell"
        }
        // A line using shell syntax, such as a redirect, stays a shell
        // command with only its executable rewritten: the exec form would
        // hand "2>/dev/null" to yp as an argument.
        (Some(rest), None) => {
            let fresh = format!("{} {subcommand}{rest}", exe_word(target.exe, target.shell));
            if as_written == fresh.trim() {
                return None;
            }
            handler["command"] = Value::String(fresh);
            "for the shell Claude Code uses here"
        }
    };
    Some(
        rewrite_note(what, &found.exe, target.exe, how)
            .unwrap_or_else(|| format!("rewrote the {what} {how}")),
    )
}

/// True when a handler of ours is in the exec form, which needs a Claude Code
/// that reads `args`.
fn uses_exec_form(settings: &Value) -> bool {
    [("UserPromptSubmit", "hook"), ("SessionStart", "index")]
        .into_iter()
        .any(|(event, subcommand)| {
            settings["hooks"][event]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|group| group.get("hooks").and_then(Value::as_array))
                .flatten()
                .filter_map(|h| our_handler(h, subcommand))
                .any(|found| found.rest.is_none())
        })
}

/// Add our handler for `event`, or bring the one already there up to date.
fn register(
    settings: &mut Value,
    notes: &mut Notes,
    target: &Target,
    event: &str,
    args: &[&str],
    timeout: u64,
) {
    let hooks = settings
        .as_object_mut()
        .expect("settings is an object")
        .entry("hooks")
        .or_insert_with(|| json!({}));
    if !hooks.is_object() {
        *hooks = json!({});
    }
    let events = hooks
        .as_object_mut()
        .expect("hooks is an object")
        .entry(event)
        .or_insert_with(|| json!([]));
    if !events.is_array() {
        *events = json!([]);
    }
    let list = events.as_array_mut().expect("event list is an array");

    let subcommand = args[0];
    let what = format!("{event} hook");
    if list.iter().any(|group| group_has(group, subcommand)) {
        let mut changed = Vec::new();
        for group in list.iter_mut() {
            let Some(inner) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                continue;
            };
            for handler in inner.iter_mut() {
                if let Some(found) = our_handler(handler, subcommand) {
                    changed.extend(update_handler(handler, found, target, subcommand, &what));
                }
            }
        }
        if changed.is_empty() {
            notes.push(format!("{what} already registered; left as is"));
        }
        notes.extend(changed);
        return;
    }

    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let mut handler = hook_handler(target.exe, &args);
    handler["timeout"] = json!(timeout);
    list.push(json!({ "hooks": [handler] }));
    notes.push(format!("registered the {what}"));
}

/// True when the yourprompt plugin is switched on in these settings. Its
/// hooks.json registers the same hooks, and two copies of one handler from
/// different sources both run.
fn plugin_enabled(settings: &Value) -> bool {
    settings
        .get("enabledPlugins")
        .and_then(Value::as_object)
        .is_some_and(|plugins| {
            plugins
                .iter()
                .any(|(id, on)| id.starts_with("yourprompt@") && on.as_bool() == Some(true))
        })
}

/// Remove everything [`apply`] added, putting back the status line it
/// wrapped exactly as it was.
pub fn remove(settings: &mut Value, saved: &mut Saved) -> Notes {
    let mut notes = Notes::new();
    let Some(object) = settings.as_object_mut() else {
        return notes;
    };

    // ---- status line --------------------------------------------------
    let ours_rest = object
        .get("statusLine")
        .and_then(|s| s.get("command"))
        .and_then(Value::as_str)
        .and_then(|c| ours(c, "statusline"))
        .map(|(_, rest)| rest.to_string());
    match ours_rest {
        Some(rest) => {
            let restored = saved.status_line.take().or_else(|| {
                let original = legacy_wrap(&rest)?;
                let mut line = object["statusLine"].clone();
                line["command"] = Value::String(original);
                Some(line)
            });
            match restored {
                Some(line) => {
                    let shown = line
                        .get("command")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    object.insert("statusLine".into(), line);
                    notes.push(format!("restored your status line: {shown}"));
                }
                None => {
                    object.shift_remove("statusLine");
                    notes.push("removed the status line".into());
                }
            }
        }
        // Not ours any more: someone replaced it since, and what was kept no
        // longer stands behind anything.
        None => saved.status_line = None,
    }

    // ---- hooks --------------------------------------------------------
    for (event, subcommand) in [("UserPromptSubmit", "hook"), ("SessionStart", "index")] {
        unregister(object, &mut notes, event, subcommand);
    }

    notes
}

/// Take our handlers out of `event`, and whatever that leaves empty.
fn unregister(settings: &mut Map<String, Value>, notes: &mut Notes, event: &str, subcommand: &str) {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return;
    };
    let Some(groups) = hooks.get_mut(event).and_then(Value::as_array_mut) else {
        return;
    };
    let mut removed = 0;
    groups.retain_mut(|group| {
        let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
            return true;
        };
        let before = handlers.len();
        handlers.retain(|h| our_handler(h, subcommand).is_none());
        let gone = before - handlers.len();
        removed += gone;
        // A group left with nothing was ours alone. One still holding
        // someone else's handler stays.
        gone == 0 || !handlers.is_empty()
    });
    if removed == 0 {
        return;
    }
    notes.push(format!("removed the {event} hook"));
    if groups.is_empty() {
        hooks.shift_remove(event);
    }
    if hooks.is_empty() {
        settings.shift_remove("hooks");
    }
}

/// What to do about the `/score` command file.
#[derive(Debug, PartialEq, Eq)]
enum CommandFile {
    Write,
    Delete,
    Leave,
}

/// Decide what happens to `commands/score.md`, given what is there now.
///
/// A file is ours when it is the one this version writes, or the one an
/// earlier `yp install` recorded writing. Anything else is the user's: their
/// own `/score`, or ours edited, and is never overwritten or deleted.
fn plan_command(
    existing: Option<&str>,
    saved: &mut Saved,
    uninstall: bool,
    shown: &str,
    notes: &mut Notes,
) -> CommandFile {
    let ours = |text: &str| text == SCORE_COMMAND || saved.score_command.as_deref() == Some(text);
    if uninstall {
        let plan = match existing {
            Some(text) if ours(text) => {
                notes.push("removed the /score command".into());
                CommandFile::Delete
            }
            Some(_) => {
                notes.push(format!("left {shown} alone: yp install did not write it"));
                CommandFile::Leave
            }
            None => CommandFile::Leave,
        };
        saved.score_command = None;
        return plan;
    }
    let plan = match existing {
        Some(text) if text == SCORE_COMMAND => {
            notes.push("/score command already there; left as is".into());
            CommandFile::Leave
        }
        Some(text) if !ours(text) => {
            notes.push(format!(
                "left your own {shown} alone, so /score is still yours; \
                 `yp explain` prints the breakdown"
            ));
            return CommandFile::Leave;
        }
        Some(_) => {
            notes.push(format!("updated the /score command in {shown}"));
            CommandFile::Write
        }
        None => {
            notes.push(format!("added the /score command: {shown}"));
            CommandFile::Write
        }
    };
    saved.score_command = Some(SCORE_COMMAND.to_string());
    plan
}

/// Where `yp install` keeps [`Saved`].
fn saved_path() -> Option<PathBuf> {
    session::state_dir().map(|d| d.join("install.json"))
}

fn load_saved() -> Saved {
    saved_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Write [`Saved`], or remove the file once there is nothing left in it.
fn store(saved: &Saved) -> io::Result<()> {
    let path = saved_path().ok_or_else(|| io::Error::other("no state directory available"))?;
    if *saved == Saved::default() {
        return match fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_json::to_string_pretty(saved)? + "\n")
}

/// The status line `yp install` wrapped, for `yp statusline --wrapped`.
pub fn wrapped_status_line() -> Option<String> {
    let line = load_saved().status_line?;
    line.get("command")?.as_str().map(str::to_string)
}

/// True when the status line in `settings` is ours, running one kept in
/// [`Saved`].
fn runs_a_kept_status_line(settings: &Value) -> bool {
    settings["statusLine"]["command"]
        .as_str()
        .and_then(|command| ours(command, "statusline"))
        .is_some_and(|(_, rest)| runs_kept(rest))
}

/// The status line in the backup of the settings from before yp first
/// changed them, if it is not one of ours.
fn backed_up_status_line(backup: &Path) -> Option<Value> {
    let text = fs::read_to_string(backup).ok()?;
    let settings: Value =
        serde_json::from_str(text.strip_prefix('\u{feff}').unwrap_or(&text)).ok()?;
    let line = settings.get("statusLine")?;
    let command = line
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default();
    ours(command, "statusline").is_none().then(|| line.clone())
}

/// True when a shell would find `yp` by name, which `/score` needs: it runs
/// `yp explain`.
fn yp_on_path() -> bool {
    let name = if cfg!(windows) { "yp.exe" } else { "yp" };
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(name).is_file()))
}

fn settings_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(dir).join("settings.json"));
    }
    session::home_dir().map(|h| h.join(".claude").join("settings.json"))
}

fn current_exe() -> String {
    std::env::current_exe()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
        // If we cannot find ourselves, fall back to the bare name and trust
        // PATH. Better a command the user can fix than no command at all.
        .unwrap_or_else(|| "yp".to_string())
}

pub fn run(print_only: bool, uninstall: bool) -> ExitCode {
    let Some(path) = settings_path() else {
        eprintln!("yp: could not locate your Claude Code settings directory");
        eprintln!("    set CLAUDE_CONFIG_DIR or HOME and try again");
        return ExitCode::FAILURE;
    };

    // A file that is there but unreadable must stop the install: read as
    // missing, it would be replaced, with no backup, by yp's lines alone.
    let raw = match fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => {
            eprintln!("yp: could not read {}: {e}", path.display());
            eprintln!("    it has to be UTF-8 JSON; fix or move it, then run this again");
            return ExitCode::FAILURE;
        }
    };
    // Editors on Windows like to start a file with a byte order mark, which
    // JSON parsers refuse.
    let text = raw
        .as_deref()
        .map(|r| r.strip_prefix('\u{feff}').unwrap_or(r))
        .filter(|r| !r.trim().is_empty())
        .unwrap_or("{}");
    let mut settings: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("yp: {} is not valid JSON: {e}", path.display());
            eprintln!("    fix or move it, then run this again");
            return ExitCode::FAILURE;
        }
    };
    let original = settings.clone();
    let saved_before = load_saved();
    let mut saved = saved_before.clone();
    let backup = path.with_extension("json.yp-backup");
    let exe = current_exe();

    let mut notes = Notes::new();
    if saved.status_line.is_none() && runs_a_kept_status_line(&settings) {
        // What yp kept of the status line it wraps is gone, as when its
        // state directory was cleared. The backup still holds the one from
        // before the first install, which beats losing it.
        if let Some(line) = backed_up_status_line(&backup) {
            notes.push(format!(
                "nothing was kept of the status line yp wrapped, so it comes from {}",
                backup.display()
            ));
            saved.status_line = Some(line);
        }
    }
    notes.extend(if uninstall {
        remove(&mut settings, &mut saved)
    } else {
        apply(&mut settings, &exe, Shell::here(), &mut saved)
    });

    // The commands directory sits beside settings.json, wherever
    // CLAUDE_CONFIG_DIR puts it.
    let command_path = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("commands")
        .join("score.md");
    let existing_command = fs::read_to_string(&command_path).ok();
    let command_plan = plan_command(
        existing_command.as_deref(),
        &mut saved,
        uninstall,
        &command_path.display().to_string(),
        &mut notes,
    );
    let score_is_ours = match command_plan {
        CommandFile::Write => true,
        CommandFile::Leave => existing_command.as_deref() == Some(SCORE_COMMAND),
        CommandFile::Delete => false,
    };
    if !uninstall && score_is_ours && !yp_on_path() {
        let dir = Path::new(&exe)
            .parent()
            .map(|dir| dir.display().to_string())
            .unwrap_or_default();
        notes.push(format!(
            "/score runs `yp explain`, and there is no yp on your PATH to run: add {dir} \
             to PATH, or /score will fail"
        ));
    }

    let rendered = match serde_json::to_string_pretty(&settings) {
        Ok(s) => s + "\n",
        Err(e) => {
            eprintln!("yp: could not serialise settings: {e}");
            return ExitCode::FAILURE;
        }
    };

    if print_only {
        // Everything but the settings is printed as comments, so the output
        // with the `#` lines dropped is still the JSON that would be written.
        println!("# would write {}", path.display());
        for note in &notes {
            println!("# - {note}");
        }
        print!("{rendered}");
        match command_plan {
            CommandFile::Write => {
                println!("# would write {}:", command_path.display());
                for line in SCORE_COMMAND.lines() {
                    if line.is_empty() {
                        println!("#");
                    } else {
                        println!("#   {line}");
                    }
                }
            }
            CommandFile::Delete => println!("# would remove {}", command_path.display()),
            CommandFile::Leave => {}
        }
        if let (Some(line), Some(kept)) = (&saved.status_line, saved_path()) {
            if saved.status_line != saved_before.status_line {
                println!(
                    "# would keep your status line in {}: {line}",
                    kept.display()
                );
            }
        }
        return ExitCode::SUCCESS;
    }

    let changed = settings != original;

    if !uninstall {
        // One backup, of the settings as they were before yp first changed
        // them. Writing it on every run replaced it with yp's own output the
        // second time round, which left nothing to go back to.
        let worth_keeping = raw
            .as_deref()
            .is_some_and(|r| !matches!(r.trim(), "" | "{}"));
        if changed && worth_keeping {
            if backup.exists() {
                println!("kept your earlier backup: {}", backup.display());
            } else if let Err(e) = fs::write(&backup, raw.as_deref().unwrap_or_default()) {
                eprintln!("yp: could not write a backup to {}: {e}", backup.display());
                eprintln!("    refusing to modify your settings without one");
                return ExitCode::FAILURE;
            } else {
                println!("backed up your settings to {}", backup.display());
            }
        }
        // Kept before the settings point at it, so the status line never
        // runs `--wrapped` with nothing to run.
        if saved != saved_before {
            if let Err(e) = store(&saved) {
                eprintln!("yp: could not save what it keeps in its state directory: {e}");
                eprintln!("    refusing to modify your settings without it");
                return ExitCode::FAILURE;
            }
        }
        if let (Some(_), Some(kept)) = (&saved.status_line, saved_path()) {
            if saved.status_line != saved_before.status_line {
                notes.push(format!(
                    "your status line is kept in {}, and runs from there",
                    kept.display()
                ));
            }
        }
    }

    if changed {
        if let Some(parent) = path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                eprintln!("yp: could not create {}: {e}", parent.display());
                return ExitCode::FAILURE;
            }
        }
        if let Err(e) = fs::write(&path, rendered) {
            eprintln!("yp: could not write {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
        println!("updated {}", path.display());
    } else {
        println!("{} needed no change", path.display());
    }

    // `/score` is a convenience on top of the settings, so failing to write
    // it is said, not fatal.
    let command_result = match command_plan {
        CommandFile::Write => command_path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(&command_path, SCORE_COMMAND)),
        CommandFile::Delete => fs::remove_file(&command_path),
        CommandFile::Leave => Ok(()),
    };
    if let Err(e) = command_result {
        eprintln!("yp: could not update {}: {e}", command_path.display());
    }
    if uninstall && saved != saved_before {
        if let Err(e) = store(&saved) {
            eprintln!("yp: could not update what it keeps in its state directory: {e}");
        }
    }

    for note in notes {
        println!("  - {note}");
    }
    if uninstall {
        if backup.exists() {
            println!(
                "\nYour settings from before the first yp install are still in {}",
                backup.display()
            );
        }
    } else if uses_exec_form(&settings) {
        println!(
            "\nStart a new Claude Code session to see the score. The hooks need \
             Claude Code 2.1.139 or newer."
        );
    } else {
        println!("\nStart a new Claude Code session to see the score.");
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXE: &str = "/usr/local/bin/yp";

    fn fresh() -> Value {
        json!({})
    }

    /// Install with `exe`, as Claude Code's shell on macOS and Linux.
    fn install(v: &mut Value, exe: &str, saved: &mut Saved) -> Notes {
        apply(v, exe, Shell::Posix, saved)
    }

    fn statusline_of(v: &Value) -> &str {
        v["statusLine"]["command"].as_str().unwrap()
    }

    fn handler_of<'a>(v: &'a Value, event: &str) -> &'a Value {
        &v["hooks"][event][0]["hooks"][0]
    }

    fn is_ours(command: &str, subcommand: &str) -> bool {
        ours(command, subcommand).is_some()
    }

    #[test]
    fn registers_the_hooks_in_the_exec_form_and_the_status_line() {
        let mut v = fresh();
        install(&mut v, EXE, &mut Saved::default());
        assert_eq!(statusline_of(&v), "/usr/local/bin/yp statusline");
        assert_eq!(
            *handler_of(&v, "UserPromptSubmit"),
            json!({
                "type": "command",
                "command": "/usr/local/bin/yp",
                "args": ["hook"],
                "timeout": HOOK_TIMEOUT
            })
        );
        assert_eq!(
            *handler_of(&v, "SessionStart"),
            json!({
                "type": "command",
                "command": "/usr/local/bin/yp",
                "args": ["index", "--quiet", "--max-age", "900"],
                "timeout": INDEX_TIMEOUT
            })
        );
        assert!(uses_exec_form(&v));
    }

    #[test]
    fn windows_paths_are_written_with_forward_slashes() {
        // v0.1.1 wrote "C:\Users\...\yp.exe hook"; Git Bash ran
        // "C:UsersCOMPUTER...yp.exe" and exited 127 on every prompt.
        let mut v = fresh();
        install(
            &mut v,
            r"C:\Users\me\.cargo\bin\yp.exe",
            &mut Saved::default(),
        );
        assert_eq!(
            statusline_of(&v),
            "C:/Users/me/.cargo/bin/yp.exe statusline"
        );
        for event in ["UserPromptSubmit", "SessionStart"] {
            assert_eq!(
                handler_of(&v, event)["command"],
                "C:/Users/me/.cargo/bin/yp.exe"
            );
        }
        assert!(!v.to_string().contains('\\'), "got {v}");
    }

    #[test]
    fn a_path_needing_quotes_is_quoted_for_the_shell_that_reads_the_status_line() {
        let exe = r"C:\Users\John Smith\AppData\Local\Programs\yourprompt\yp.exe";
        let unquoted = "C:/Users/John Smith/AppData/Local/Programs/yourprompt/yp.exe";
        for (shell, status_line) in [
            (Shell::Posix, format!("'{unquoted}' statusline")),
            // PowerShell runs a quoted path only behind `&`.
            (Shell::PowerShell, format!("& '{unquoted}' statusline")),
        ] {
            let mut v = fresh();
            apply(&mut v, exe, shell, &mut Saved::default());
            assert_eq!(statusline_of(&v), status_line, "{shell:?}");
            // No shell reads the hooks, so their path is never quoted.
            let hook = handler_of(&v, "UserPromptSubmit");
            assert_eq!(hook["command"], unquoted, "{shell:?}");
            assert_eq!(hook["args"], json!(["hook"]), "{shell:?}");
        }
        // A path that needs nothing is written the same for both.
        assert_eq!(
            statusline_command(r"C:\yp\yp.exe", Shell::PowerShell, true),
            "C:/yp/yp.exe statusline --wrapped"
        );
        assert_eq!(
            statusline_command(r"C:\yp\yp.exe", Shell::Posix, true),
            "C:/yp/yp.exe statusline --wrapped"
        );
    }

    #[test]
    fn the_status_line_follows_the_shell_when_it_changes() {
        // Installed without Git Bash, then Git for Windows arrives.
        let exe = r"C:\Program Files\yp\yp.exe";
        let mut v = fresh();
        let mut saved = Saved::default();
        let notes = apply(&mut v, exe, Shell::PowerShell, &mut saved);
        assert!(notes
            .iter()
            .any(|n| n.contains("Git Bash is not installed")));
        assert_eq!(
            statusline_of(&v),
            "& 'C:/Program Files/yp/yp.exe' statusline"
        );
        let hooks_before = v["hooks"].clone();

        let notes = apply(&mut v, exe, Shell::Posix, &mut saved);
        assert_eq!(statusline_of(&v), "'C:/Program Files/yp/yp.exe' statusline");
        assert!(
            notes.iter().any(|n| n.contains("for the shell")),
            "{notes:?}"
        );
        assert_eq!(v["hooks"], hooks_before, "the hooks need no shell");
    }

    #[test]
    fn a_quote_in_the_path_survives_both_shells() {
        let exe = r"C:\Users\O'Brien\yp.exe";
        assert_eq!(exe_word(exe, Shell::Posix), r"'C:/Users/O'\''Brien/yp.exe'");
        assert_eq!(
            exe_word(exe, Shell::PowerShell),
            "& 'C:/Users/O''Brien/yp.exe'"
        );
        for shell in [Shell::Posix, Shell::PowerShell] {
            let command = statusline_command(exe, shell, false);
            assert_eq!(
                split_exe(&command).0,
                "C:/Users/O'Brien/yp.exe",
                "{command}"
            );
            assert!(is_ours(&command, "statusline"), "{command}");
        }
    }

    #[test]
    fn a_verbatim_or_unc_path_is_written_so_a_shell_can_run_it() {
        assert_eq!(portable(r"\\?\C:\yp\yp.exe"), "C:/yp/yp.exe");
        assert_eq!(
            portable(r"\\?\UNC\server\share\yp.exe"),
            "//server/share/yp.exe"
        );
        // A Unix path is left exactly as it is.
        assert_eq!(portable("/opt/a\\b/yp"), "/opt/a\\b/yp");
    }

    #[test]
    fn wraps_an_existing_status_line_instead_of_replacing_it() {
        let original = json!({ "type": "command", "command": "~/bin/my-statusline.sh" });
        let mut v = json!({ "statusLine": original.clone() });
        let mut saved = Saved::default();
        install(&mut v, EXE, &mut saved);
        assert_eq!(statusline_of(&v), "/usr/local/bin/yp statusline --wrapped");
        assert_eq!(saved.status_line, Some(original));
    }

    #[test]
    fn a_wrapped_status_line_is_kept_out_of_the_command_line() {
        // Each of these was garbled or emptied by being quoted into ours.
        for original in [
            r#"echo "[$(basename "$PWD")]""#,
            r#"input=$(cat); echo "mine:${#input}""#,
            r#"node -e "console.log('node-mine')""#,
            "echo `date` $HOME",
        ] {
            let mut v = json!({ "statusLine": { "type": "command", "command": original } });
            let mut saved = Saved::default();
            install(&mut v, EXE, &mut saved);
            assert_eq!(statusline_of(&v), "/usr/local/bin/yp statusline --wrapped");
            assert_eq!(
                saved.status_line.as_ref().unwrap()["command"],
                original,
                "{original}"
            );
        }
    }

    #[test]
    fn the_wrapped_status_line_keeps_its_other_fields() {
        // v0.1.1 rewrote the object as just {type, command}, losing padding.
        let original = json!({
            "type": "command",
            "command": "bash ~/.claude/statusline-command.sh",
            "padding": 0,
            "refreshInterval": 5
        });
        let mut v = json!({ "statusLine": original.clone() });
        let mut saved = Saved::default();
        install(&mut v, EXE, &mut saved);
        assert_eq!(v["statusLine"]["padding"], 0);
        assert_eq!(v["statusLine"]["refreshInterval"], 5);
        remove(&mut v, &mut saved);
        assert_eq!(v["statusLine"], original);
    }

    #[test]
    fn preserves_unrelated_settings() {
        let mut v = json!({
            "model": "opus",
            "env": { "FOO": "bar" },
            "permissions": { "allow": ["Bash(ls:*)"] }
        });
        install(&mut v, EXE, &mut Saved::default());
        assert_eq!(v["model"], "opus");
        assert_eq!(v["env"]["FOO"], "bar");
        assert_eq!(v["permissions"]["allow"][0], "Bash(ls:*)");
    }

    #[test]
    fn keeps_other_user_prompt_submit_hooks() {
        let mut v = json!({
            "hooks": {
                "UserPromptSubmit": [
                    { "hooks": [ { "type": "command", "command": "other-tool.sh" } ] }
                ]
            }
        });
        install(&mut v, EXE, &mut Saved::default());
        let list = v["hooks"]["UserPromptSubmit"].as_array().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0]["hooks"][0]["command"], "other-tool.sh");
    }

    #[test]
    fn installing_twice_changes_nothing_the_second_time() {
        let mut v = json!({ "statusLine": { "type": "command", "command": "echo mine" } });
        let mut saved = Saved::default();
        install(&mut v, EXE, &mut saved);
        let (after_first, saved_first) = (v.clone(), saved.clone());
        let notes = install(&mut v, EXE, &mut saved);
        assert_eq!(v, after_first, "second install was not a no-op");
        assert_eq!(saved, saved_first, "second install changed what is kept");
        assert_eq!(
            notes.iter().filter(|n| n.contains("already")).count(),
            3,
            "{notes:?}"
        );
    }

    #[test]
    fn installing_twice_does_not_wrap_our_own_status_line() {
        let mut v = fresh();
        let mut saved = Saved::default();
        install(&mut v, EXE, &mut saved);
        install(&mut v, EXE, &mut saved);
        assert!(
            !statusline_of(&v).contains("--wrap"),
            "got {}",
            statusline_of(&v)
        );
        assert_eq!(saved.status_line, None);
    }

    #[test]
    fn install_install_uninstall_gives_back_the_original_settings() {
        // The reported case: padding was lost, the backup was overwritten,
        // and uninstall left "SessionStart": [] behind.
        let original = json!({
            "model": "opus",
            "statusLine": {
                "type": "command",
                "command": "bash ~/.claude/statusline-command.sh",
                "padding": 0
            },
            "hooks": {
                "UserPromptSubmit": [
                    { "hooks": [ { "type": "command", "command": "echo other-hook" } ] }
                ]
            }
        });
        let mut v = original.clone();
        let mut saved = Saved::default();
        install(&mut v, EXE, &mut saved);
        install(&mut v, EXE, &mut saved);
        remove(&mut v, &mut saved);
        assert_eq!(v, original);
        // Key order too, which serde_json keeps.
        assert_eq!(v.to_string(), original.to_string());
        assert_eq!(saved, Saved::default());
    }

    #[test]
    fn uninstall_after_a_fresh_install_leaves_nothing_behind() {
        let mut v = fresh();
        let mut saved = Saved::default();
        install(&mut v, EXE, &mut saved);
        let notes = remove(&mut v, &mut saved);
        assert_eq!(v, fresh(), "{notes:?}");
    }

    #[test]
    fn uninstall_restores_a_wrapped_status_line_exactly() {
        let original = "~/bin/my-statusline.sh --flag";
        let mut v = json!({
            "statusLine": { "type": "command", "command": original }
        });
        let mut saved = Saved::default();
        install(&mut v, EXE, &mut saved);
        remove(&mut v, &mut saved);
        assert_eq!(statusline_of(&v), original);
    }

    #[test]
    fn uninstall_removes_a_status_line_we_added_outright() {
        let mut v = fresh();
        let mut saved = Saved::default();
        install(&mut v, EXE, &mut saved);
        remove(&mut v, &mut saved);
        assert!(v.get("statusLine").is_none(), "got {v}");
    }

    #[test]
    fn uninstall_leaves_other_hooks_alone() {
        let mut v = json!({
            "hooks": {
                "UserPromptSubmit": [
                    { "hooks": [ { "type": "command", "command": "other-tool.sh" } ] }
                ]
            }
        });
        let mut saved = Saved::default();
        install(&mut v, EXE, &mut saved);
        remove(&mut v, &mut saved);
        let list = v["hooks"]["UserPromptSubmit"].as_array().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["hooks"][0]["command"], "other-tool.sh");
        assert!(v["hooks"].get("SessionStart").is_none(), "got {v}");
    }

    #[test]
    fn uninstall_keeps_a_handler_sharing_a_group_with_ours() {
        let mut v = json!({
            "hooks": {
                "UserPromptSubmit": [{ "hooks": [
                    { "type": "command", "command": "/usr/local/bin/yp hook" },
                    { "type": "command", "command": "other-tool.sh" }
                ] }]
            }
        });
        remove(&mut v, &mut Saved::default());
        assert_eq!(
            v["hooks"]["UserPromptSubmit"],
            json!([{ "hooks": [ { "type": "command", "command": "other-tool.sh" } ] }])
        );
    }

    #[test]
    fn uninstall_removes_hooks_in_either_form() {
        let mut v = json!({
            "hooks": {
                "UserPromptSubmit": [
                    { "hooks": [ { "type": "command", "command": "C:/yp/yp.exe", "args": ["hook"] } ] },
                    { "hooks": [ { "type": "command", "command": r"C:\yp\yp.exe hook" } ] }
                ],
                "SessionStart": [
                    { "hooks": [ { "type": "command", "command": "/usr/local/bin/yp index --quiet --max-age 900" } ] }
                ]
            }
        });
        remove(&mut v, &mut Saved::default());
        assert_eq!(v, fresh());
    }

    #[test]
    fn a_wrapped_command_containing_quotes_round_trips() {
        let original = r#"sh -c "echo \"hi\"""#;
        let mut v = json!({ "statusLine": { "type": "command", "command": original } });
        let mut saved = Saved::default();
        install(&mut v, EXE, &mut saved);
        remove(&mut v, &mut saved);
        assert_eq!(statusline_of(&v), original);
    }

    #[test]
    fn every_form_yp_has_written_is_recognised_as_ours_on_every_platform() {
        // Must hold regardless of which platform is doing the reading: a
        // Windows-written settings.json read from WSL must not be mistaken
        // for someone else's status line and wrapped a second time.
        for command in [
            r"C:\Users\me\bin\yp.exe statusline",
            "/usr/local/bin/yp statusline",
            r#""C:\Program Files\yp\yp.exe" statusline --wrap "echo \"hi\"""#,
            "'C:/Program Files/yp/yp.exe' statusline --wrapped",
            "& 'C:/Program Files/yp/yp.exe' statusline",
            r"/opt/my\ tools/yp statusline",
            "yp statusline",
        ] {
            assert!(is_ours(command, "statusline"), "{command}");
        }
        assert!(is_ours(r#""C:\Program Files\yp\yp.exe" hook"#, "hook"));
        let exec = json!({ "type": "command", "command": "C:/yp/yp.exe", "args": ["hook"] });
        assert!(our_handler(&exec, "hook").is_some());
        assert!(our_handler(&exec, "index").is_none());
    }

    #[test]
    fn an_unrelated_command_mentioning_statusline_is_not_ours() {
        assert!(!is_ours("~/bin/my-statusline.sh", "statusline"));
        assert!(!is_ours("python statusline.py", "statusline"));
        assert!(!is_ours("yp statuslines", "statusline"));
        assert!(!is_ours("bash -c 'yp statusline'", "statusline"));
        // A directory named after a subcommand is not the subcommand.
        assert!(!is_ours("/opt/hook/yp statusline", "hook"));
        let theirs = json!({ "type": "command", "command": "node", "args": ["hook"] });
        assert!(our_handler(&theirs, "hook").is_none());
    }

    #[test]
    fn reinstalling_upgrades_what_v011_wrote_on_windows() {
        // What v0.1.1 wrote, read back by the fixed binary in the same place.
        let old_exe = r"C:\Users\me\.cargo\bin\yp.exe";
        let mut v = json!({
            "statusLine": {
                "type": "command",
                "command": format!(r#"{old_exe} statusline --wrap "~/.claude/statusline.sh""#),
                "padding": 1
            },
            "hooks": {
                "UserPromptSubmit": [ { "hooks": [
                    { "type": "command", "command": format!("{old_exe} hook") }
                ] } ],
                "SessionStart": [ { "hooks": [
                    { "type": "command", "command": format!("{old_exe} index --quiet --max-age 900") }
                ] } ]
            }
        });
        let mut saved = Saved::default();
        let notes = apply(&mut v, old_exe, Shell::Posix, &mut saved);

        assert_eq!(
            statusline_of(&v),
            "C:/Users/me/.cargo/bin/yp.exe statusline --wrapped"
        );
        assert_eq!(
            saved.status_line,
            Some(json!({ "type": "command", "command": "~/.claude/statusline.sh", "padding": 1 }))
        );
        assert_eq!(
            *handler_of(&v, "UserPromptSubmit"),
            json!({ "type": "command", "command": "C:/Users/me/.cargo/bin/yp.exe", "args": ["hook"] })
        );
        assert_eq!(
            handler_of(&v, "SessionStart")["args"],
            json!(["index", "--quiet", "--max-age", "900"])
        );
        for event in ["UserPromptSubmit", "SessionStart"] {
            assert_eq!(v["hooks"][event].as_array().unwrap().len(), 1, "{event}");
        }
        assert!(
            notes.iter().any(|n| n.contains("forward slashes")),
            "{notes:?}"
        );
        assert_eq!(
            notes.iter().filter(|n| n.contains("backslashes")).count(),
            3,
            "{notes:?}"
        );
        assert!(notes.iter().any(|n| n.contains("moved")), "{notes:?}");
    }

    #[test]
    fn what_v011_wrote_on_unix_is_left_as_it_is() {
        // It runs under sh, and it runs under a Claude Code too old for the
        // exec form as well.
        let mut v = json!({
            "statusLine": { "type": "command", "command": "/usr/local/bin/yp statusline" },
            "hooks": {
                "UserPromptSubmit": [ { "hooks": [
                    { "type": "command", "command": "/usr/local/bin/yp hook" }
                ] } ],
                "SessionStart": [ { "hooks": [
                    { "type": "command", "command": "/usr/local/bin/yp index --quiet --max-age 900" }
                ] } ]
            }
        });
        let before = v.clone();
        let notes = install(&mut v, EXE, &mut Saved::default());
        assert_eq!(v, before, "{notes:?}");
        assert!(!uses_exec_form(&v));
    }

    #[test]
    fn reinstalling_from_a_new_location_repoints_every_command() {
        // cargo install first, then install.ps1: the old binary is gone.
        let old = if cfg!(windows) {
            r"C:\nonexistent\old-bin\yp.exe"
        } else {
            "/nonexistent/old-bin/yp"
        };
        let mut v = json!({ "statusLine": { "type": "command", "command": "echo mine" } });
        let mut saved = Saved::default();
        install(&mut v, old, &mut saved);
        let notes = install(&mut v, EXE, &mut saved);

        assert_eq!(statusline_of(&v), "/usr/local/bin/yp statusline --wrapped");
        assert_eq!(saved.status_line.as_ref().unwrap()["command"], "echo mine");
        for event in ["UserPromptSubmit", "SessionStart"] {
            assert_eq!(handler_of(&v, event)["command"], EXE, "{event}");
            assert_eq!(v["hooks"][event].as_array().unwrap().len(), 1, "{event}");
        }
        let old = portable(old);
        assert_eq!(
            notes
                .iter()
                .filter(|n| n.contains(&old) && n.contains("no longer exists"))
                .count(),
            3,
            "{notes:?}"
        );
    }

    #[test]
    fn a_settings_file_written_on_windows_and_read_from_wsl_is_not_wrapped_again() {
        let mut v = json!({
            "statusLine": {
                "type": "command",
                "command": r"C:\Users\me\bin\yp.exe statusline"
            }
        });
        let mut saved = Saved::default();
        install(&mut v, "/usr/local/bin/yp", &mut saved);
        assert_eq!(statusline_of(&v), "/usr/local/bin/yp statusline");
        assert_eq!(saved.status_line, None);
    }

    #[test]
    fn a_handler_the_user_tuned_keeps_their_fields() {
        let mut v = json!({
            "hooks": { "SessionStart": [ { "hooks": [ {
                "type": "command",
                "command": "/old/yp index --quiet --max-age 3600",
                "timeout": 120
            } ] } ] }
        });
        install(&mut v, EXE, &mut Saved::default());
        assert_eq!(
            *handler_of(&v, "SessionStart"),
            json!({
                "type": "command",
                "command": "/usr/local/bin/yp",
                "args": ["index", "--quiet", "--max-age", "3600"],
                "timeout": 120
            })
        );
    }

    #[test]
    fn a_bare_yp_follows_path_and_is_left_alone() {
        let mut v = json!({
            "statusLine": { "type": "command", "command": "yp statusline" },
            "hooks": { "UserPromptSubmit": [ { "hooks": [
                { "type": "command", "command": "yp hook" }
            ] } ] }
        });
        let before = v.clone();
        install(&mut v, EXE, &mut Saved::default());
        assert_eq!(v["statusLine"], before["statusLine"]);
        assert_eq!(
            v["hooks"]["UserPromptSubmit"],
            before["hooks"]["UserPromptSubmit"]
        );
    }

    #[test]
    fn a_hand_written_wrap_is_kept_as_it_was_written() {
        // Only the exact `--wrap "..."` v0.1.1 wrote moves out of the command
        // line. A hand-written one used to be rewritten as a bare
        // `statusline`, and the command it named was lost.
        for (command, expected) in [
            (
                "yp statusline --wrap '~/bin/mine.sh'",
                "yp statusline --wrap '~/bin/mine.sh'",
            ),
            (
                r"C:\old\yp.exe statusline --wrap '~/bin/mine.sh'",
                "/usr/local/bin/yp statusline --wrap '~/bin/mine.sh'",
            ),
            (
                r#"/usr/local/bin/yp statusline --wrap "~/bin/mine.sh" --x"#,
                r#"/usr/local/bin/yp statusline --wrap "~/bin/mine.sh" --x"#,
            ),
        ] {
            let mut v = json!({ "statusLine": { "type": "command", "command": command } });
            let mut saved = Saved::default();
            install(&mut v, EXE, &mut saved);
            assert_eq!(statusline_of(&v), expected);
            assert_eq!(saved.status_line, None, "{command}");
        }
        assert_eq!(
            legacy_wrap(r#" --wrap "say \"hi\" \\o/""#).as_deref(),
            Some(r#"say "hi" \o/"#)
        );
        assert_eq!(legacy_wrap(" --wrapped"), None);
    }

    #[test]
    fn a_shell_form_hook_with_shell_syntax_stays_a_shell_command() {
        // The exec form would pass "2>/dev/null" to yp as an argument.
        let mut v = json!({
            "hooks": { "UserPromptSubmit": [ { "hooks": [
                { "type": "command", "command": r"C:\yp\yp.exe hook 2>/dev/null" }
            ] } ] }
        });
        install(&mut v, r"C:\yp\yp.exe", &mut Saved::default());
        let hook = handler_of(&v, "UserPromptSubmit");
        assert!(hook.get("args").is_none(), "got {hook}");
        assert_eq!(hook["command"], "C:/yp/yp.exe hook 2>/dev/null");
    }

    #[test]
    fn uninstall_removes_both_hooks_and_what_they_leave_empty() {
        let mut v = json!({ "model": "opus" });
        let mut saved = Saved::default();
        install(&mut v, EXE, &mut saved);
        remove(&mut v, &mut saved);
        assert!(v.get("hooks").is_none(), "got {v}");
        assert_eq!(v, json!({ "model": "opus" }));
    }

    #[test]
    fn a_quoted_exe_path_survives_a_reinstall_and_an_uninstall() {
        let exe = r"C:\Program Files\yp\yp.exe";
        for shell in [Shell::Posix, Shell::PowerShell] {
            let mut v = fresh();
            let mut saved = Saved::default();
            apply(&mut v, exe, shell, &mut saved);
            let after_first = v.clone();
            apply(&mut v, exe, shell, &mut saved);
            assert_eq!(v, after_first, "{shell:?}: reinstall was not a no-op");
            remove(&mut v, &mut saved);
            assert_eq!(v, fresh(), "{shell:?}");
        }
    }

    #[test]
    fn a_non_object_settings_file_is_replaced_rather_than_crashing() {
        let mut v = json!([1, 2, 3]);
        let notes = install(&mut v, EXE, &mut Saved::default());
        assert!(v.is_object());
        assert!(notes.iter().any(|n| n.contains("not a JSON object")));
    }

    #[test]
    fn malformed_hook_sections_are_repaired_rather_than_crashing() {
        let mut v = json!({ "hooks": "nonsense" });
        install(&mut v, EXE, &mut Saved::default());
        assert_eq!(handler_of(&v, "UserPromptSubmit")["command"], EXE);
        assert_eq!(handler_of(&v, "UserPromptSubmit")["args"], json!(["hook"]));
    }

    #[test]
    fn a_status_line_without_a_command_is_put_back_on_uninstall() {
        let original = json!({ "type": "command", "padding": 2 });
        let mut v = json!({ "statusLine": original.clone() });
        let mut saved = Saved::default();
        install(&mut v, EXE, &mut saved);
        assert_eq!(statusline_of(&v), "/usr/local/bin/yp statusline");
        remove(&mut v, &mut saved);
        assert_eq!(v["statusLine"], original);
    }

    #[test]
    fn a_status_line_replaced_since_the_install_is_left_alone() {
        let mut v = json!({ "statusLine": { "type": "command", "command": "echo mine" } });
        let mut saved = Saved::default();
        install(&mut v, EXE, &mut saved);
        v["statusLine"] = json!({ "type": "command", "command": "echo newer" });
        remove(&mut v, &mut saved);
        assert_eq!(statusline_of(&v), "echo newer");
        assert_eq!(saved.status_line, None);
    }

    #[test]
    fn an_enabled_plugin_is_pointed_out() {
        let mut v = json!({ "enabledPlugins": { "yourprompt@somewhere": true } });
        let notes = install(&mut v, EXE, &mut Saved::default());
        assert!(
            notes.iter().any(|n| n.contains("scored twice")),
            "{notes:?}"
        );
    }

    fn plan(existing: Option<&str>, saved: &mut Saved, uninstall: bool) -> CommandFile {
        plan_command(existing, saved, uninstall, "score.md", &mut Notes::new())
    }

    #[test]
    fn the_score_command_is_written_once_and_never_over_someone_elses() {
        let mut saved = Saved::default();
        assert_eq!(plan(None, &mut saved, false), CommandFile::Write);
        assert_eq!(saved.score_command.as_deref(), Some(SCORE_COMMAND));
        assert_eq!(
            plan(Some(SCORE_COMMAND), &mut saved, false),
            CommandFile::Leave
        );
        // Someone else's /score is theirs to keep, on install and uninstall.
        let theirs = "---\ndescription: my own score\n---\nsomething else\n";
        assert_eq!(plan(Some(theirs), &mut saved, false), CommandFile::Leave);
        assert_eq!(plan(Some(theirs), &mut saved, true), CommandFile::Leave);
        // One an earlier install wrote is still ours: updated, or removed.
        let earlier = Saved {
            score_command: Some("an older body\n".into()),
            ..Saved::default()
        };
        let older = Some("an older body\n");
        assert_eq!(plan(older, &mut earlier.clone(), false), CommandFile::Write);
        let mut uninstalling = earlier.clone();
        assert_eq!(plan(older, &mut uninstalling, true), CommandFile::Delete);
        assert_eq!(uninstalling.score_command, None);
        // This version's own file goes on uninstall even with no record.
        assert_eq!(
            plan(Some(SCORE_COMMAND), &mut Saved::default(), true),
            CommandFile::Delete
        );
    }

    #[test]
    fn the_score_command_is_the_plugins() {
        assert!(SCORE_COMMAND.contains("yp explain --no-color"));
        assert!(SCORE_COMMAND.starts_with("---"));
    }
}
