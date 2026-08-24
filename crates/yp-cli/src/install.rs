//! `yp install` -- register the hook and status line in Claude Code's
//! settings.
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
//! The transformation itself is a pure function over `serde_json::Value`, so
//! every case below is tested without going near a real settings file.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::{json, Value};

use crate::session;

/// What changed, in words, for the user to read.
pub type Notes = Vec<String>;

/// True if this command string is one we wrote.
///
/// Matches on the subcommand plus an executable whose file stem is `yp`, so
/// an unrelated status line that happens to mention "statusline" is left
/// alone.
fn is_ours(command: &str, subcommand: &str) -> bool {
    let trimmed = command.trim();
    if !trimmed.contains(subcommand) {
        return false;
    }
    let exe = first_token(trimmed);
    // Split on both separators by hand rather than using `Path::file_stem`.
    // On Unix, `Path` does not treat a backslash as a separator, so a
    // settings.json written on Windows and read from WSL would not be
    // recognised as ours -- and `yp install` would then wrap its own command
    // instead of leaving it alone. Sharing a home directory between Windows
    // and WSL is common enough to be worth handling.
    let file = exe.rsplit(['/', '\\']).next().unwrap_or(exe);
    let stem = file.split('.').next().unwrap_or(file).to_ascii_lowercase();
    stem == "yp"
}

/// The executable part of a command line, honouring a leading quoted path.
///
/// Splitting on whitespace alone is not enough: on Windows the binary very
/// often lives under a path with a space in it, such as
/// `C:\Program Files\yp\yp.exe`, and such a path has to be written quoted.
fn first_token(command: &str) -> &str {
    let trimmed = command.trim_start();
    match trimmed.strip_prefix('"') {
        Some(rest) => match rest.find('"') {
            Some(end) => &rest[..end],
            None => rest,
        },
        None => trimmed.split_whitespace().next().unwrap_or(""),
    }
}

/// Quote a command for embedding inside `--wrap "..."`.
fn quote(command: &str) -> String {
    format!("\"{}\"", command.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Quote the executable path if it needs it.
///
/// Written unquoted, `C:\Program Files\yp\yp.exe hook` would be parsed by the
/// shell as the command `C:\Program` with the arguments `Files\yp\yp.exe` and
/// `hook`, and the hook would silently never run.
fn quote_exe(exe: &str) -> String {
    if exe.contains(char::is_whitespace) {
        format!("\"{exe}\"")
    } else {
        exe.to_string()
    }
}

fn hook_command(exe: &str) -> String {
    format!("{} hook", quote_exe(exe))
}

fn index_command(exe: &str) -> String {
    // --max-age makes an unconditional session-start hook cheap: a session
    // opened minutes after the last one reuses the existing index.
    format!("{} index --quiet --max-age 900", quote_exe(exe))
}

fn statusline_command(exe: &str, wrapping: Option<&str>) -> String {
    match wrapping {
        Some(existing) => format!("{} statusline --wrap {}", quote_exe(exe), quote(existing)),
        None => format!("{} statusline", quote_exe(exe)),
    }
}

/// Add our hook and status line to `settings`, returning what changed.
pub fn apply(settings: &mut Value, exe: &str) -> Notes {
    let mut notes = Notes::new();
    if !settings.is_object() {
        *settings = json!({});
        notes.push("settings file was not a JSON object; started from an empty one".into());
    }

    // ---- status line --------------------------------------------------
    let existing_command = settings
        .get("statusLine")
        .and_then(|s| s.get("command"))
        .and_then(Value::as_str)
        .map(str::to_string);

    match existing_command {
        Some(command) if is_ours(&command, "statusline") => {
            notes.push("status line already registered; left as is".into());
        }
        Some(command) => {
            settings["statusLine"] = json!({
                "type": "command",
                "command": statusline_command(exe, Some(&command)),
            });
            notes.push(format!(
                "wrapped your existing status line so it keeps working: {command}"
            ));
        }
        None => {
            settings["statusLine"] = json!({
                "type": "command",
                "command": statusline_command(exe, None),
            });
            notes.push("registered the status line".into());
        }
    }

    // ---- hooks --------------------------------------------------------
    // Scoring on every prompt, and building the repository index once per
    // session. The index build is deliberately *not* on the prompt path:
    // indexing takes seconds and nothing may delay a prompt.
    add_hook(
        settings,
        &mut notes,
        "UserPromptSubmit",
        "hook",
        hook_command(exe),
    );
    add_hook(
        settings,
        &mut notes,
        "SessionStart",
        "index",
        index_command(exe),
    );

    notes
}

/// Add one hook entry for `event`, unless one of ours is already there.
fn add_hook(
    settings: &mut Value,
    notes: &mut Notes,
    event: &str,
    subcommand: &str,
    command: String,
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

    if list.iter().any(|group| group_is_ours(group, subcommand)) {
        notes.push(format!("{event} hook already registered; left as is"));
        return;
    }
    list.push(json!({
        "hooks": [ { "type": "command", "command": command } ]
    }));
    notes.push(format!("registered the {event} hook"));
}

fn group_is_ours(group: &Value, subcommand: &str) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|inner| {
            inner.iter().any(|h| {
                h.get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|c| is_ours(c, subcommand))
            })
        })
}

/// Remove everything [`apply`] added, restoring a wrapped status line.
pub fn remove(settings: &mut Value) -> Notes {
    let mut notes = Notes::new();
    if !settings.is_object() {
        return notes;
    }

    // ---- status line --------------------------------------------------
    let command = settings
        .get("statusLine")
        .and_then(|s| s.get("command"))
        .and_then(Value::as_str)
        .map(str::to_string);

    if let Some(command) = command.filter(|c| is_ours(c, "statusline")) {
        match unwrap_inner(&command) {
            // We wrapped something; put the original back exactly as it was.
            Some(original) => {
                settings["statusLine"] = json!({ "type": "command", "command": original });
                notes.push("restored the status line we had wrapped".into());
            }
            None => {
                settings
                    .as_object_mut()
                    .expect("settings is an object")
                    .remove("statusLine");
                notes.push("removed the status line".into());
            }
        }
    }

    // ---- hooks --------------------------------------------------------
    for (event, subcommand) in [("UserPromptSubmit", "hook"), ("SessionStart", "index")] {
        if let Some(list) = settings
            .get_mut("hooks")
            .and_then(|h| h.get_mut(event))
            .and_then(Value::as_array_mut)
        {
            let before = list.len();
            list.retain(|group| !group_is_ours(group, subcommand));
            if list.len() != before {
                notes.push(format!("removed the {event} hook"));
            }
        }
    }

    notes
}

/// Pull the original command back out of `yp statusline --wrap "..."`.
fn unwrap_inner(command: &str) -> Option<String> {
    let rest = command.split_once("--wrap")?.1.trim();
    let inner = rest.strip_prefix('"')?;
    // Walk the quoted string honouring backslash escapes, so a wrapped
    // command containing quotes round-trips intact.
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push(chars.next()?),
            '"' => return Some(out),
            other => out.push(other),
        }
    }
    None
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

    let raw = fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_string());
    let mut settings: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("yp: {} is not valid JSON: {e}", path.display());
            eprintln!("    fix or move it, then run this again");
            return ExitCode::FAILURE;
        }
    };

    let exe = current_exe();
    let notes = if uninstall {
        remove(&mut settings)
    } else {
        apply(&mut settings, &exe)
    };

    let rendered = match serde_json::to_string_pretty(&settings) {
        Ok(s) => s + "\n",
        Err(e) => {
            eprintln!("yp: could not serialise settings: {e}");
            return ExitCode::FAILURE;
        }
    };

    if print_only {
        println!("# would write {}", path.display());
        for note in &notes {
            println!("# - {note}");
        }
        print!("{rendered}");
        return ExitCode::SUCCESS;
    }

    if raw.trim() != "{}" && path.exists() {
        let backup = path.with_extension("json.yp-backup");
        if let Err(e) = fs::write(&backup, &raw) {
            eprintln!("yp: could not write a backup to {}: {e}", backup.display());
            eprintln!("    refusing to modify your settings without one");
            return ExitCode::FAILURE;
        }
        println!("backed up your settings to {}", backup.display());
    }

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
    for note in notes {
        println!("  - {note}");
    }
    if !uninstall {
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

    fn statusline_of(v: &Value) -> &str {
        v["statusLine"]["command"].as_str().unwrap()
    }

    #[test]
    fn registers_hook_and_status_line_in_empty_settings() {
        let mut v = fresh();
        apply(&mut v, EXE);
        assert_eq!(statusline_of(&v), "/usr/local/bin/yp statusline");
        assert_eq!(
            v["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"],
            "/usr/local/bin/yp hook"
        );
    }

    #[test]
    fn wraps_an_existing_status_line_instead_of_replacing_it() {
        let mut v = json!({
            "statusLine": { "type": "command", "command": "~/bin/my-statusline.sh" }
        });
        apply(&mut v, EXE);
        assert_eq!(
            statusline_of(&v),
            "/usr/local/bin/yp statusline --wrap \"~/bin/my-statusline.sh\""
        );
    }

    #[test]
    fn preserves_unrelated_settings() {
        let mut v = json!({
            "model": "opus",
            "env": { "FOO": "bar" },
            "permissions": { "allow": ["Bash(ls:*)"] }
        });
        apply(&mut v, EXE);
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
        apply(&mut v, EXE);
        let list = v["hooks"]["UserPromptSubmit"].as_array().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0]["hooks"][0]["command"], "other-tool.sh");
    }

    #[test]
    fn installing_twice_changes_nothing_the_second_time() {
        let mut v = fresh();
        apply(&mut v, EXE);
        let after_first = v.clone();
        let notes = apply(&mut v, EXE);
        assert_eq!(v, after_first, "second install was not a no-op");
        assert!(notes.iter().any(|n| n.contains("already")), "{notes:?}");
    }

    #[test]
    fn installing_twice_does_not_wrap_our_own_status_line() {
        let mut v = fresh();
        apply(&mut v, EXE);
        apply(&mut v, EXE);
        assert!(
            !statusline_of(&v).contains("--wrap"),
            "got {}",
            statusline_of(&v)
        );
    }

    #[test]
    fn uninstall_restores_a_wrapped_status_line_exactly() {
        let original = "~/bin/my-statusline.sh --flag";
        let mut v = json!({
            "statusLine": { "type": "command", "command": original }
        });
        apply(&mut v, EXE);
        remove(&mut v);
        assert_eq!(statusline_of(&v), original);
    }

    #[test]
    fn uninstall_removes_a_status_line_we_added_outright() {
        let mut v = fresh();
        apply(&mut v, EXE);
        remove(&mut v);
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
        apply(&mut v, EXE);
        remove(&mut v);
        let list = v["hooks"]["UserPromptSubmit"].as_array().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["hooks"][0]["command"], "other-tool.sh");
    }

    #[test]
    fn a_wrapped_command_containing_quotes_round_trips() {
        let original = r#"sh -c "echo \"hi\"""#;
        let mut v = json!({ "statusLine": { "type": "command", "command": original } });
        apply(&mut v, EXE);
        remove(&mut v);
        assert_eq!(statusline_of(&v), original);
    }

    #[test]
    fn a_windows_exe_path_is_recognised_as_ours_on_every_platform() {
        // Must hold regardless of which platform is doing the reading: a
        // Windows-written settings.json read from WSL must not be mistaken
        // for someone else's status line and wrapped a second time.
        assert!(is_ours(r"C:\Users\me\bin\yp.exe statusline", "statusline"));
        assert!(is_ours("/usr/local/bin/yp statusline", "statusline"));
        assert!(is_ours("yp hook", "hook"));
        assert!(is_ours(r#""C:\Program Files\yp\yp.exe" hook"#, "hook"));
    }

    #[test]
    fn a_double_install_across_platforms_does_not_wrap_our_own_command() {
        // The bug the separator fix prevents: settings written by the Windows
        // binary, then `yp install` run again from WSL.
        let mut v = json!({
            "statusLine": {
                "type": "command",
                "command": r"C:\Users\me\bin\yp.exe statusline"
            }
        });
        apply(&mut v, "/usr/local/bin/yp");
        assert_eq!(statusline_of(&v), r"C:\Users\me\bin\yp.exe statusline");
    }

    #[test]
    fn an_unrelated_command_mentioning_statusline_is_not_ours() {
        assert!(!is_ours("~/bin/my-statusline.sh", "statusline"));
        assert!(!is_ours("python statusline.py", "statusline"));
    }

    #[test]
    fn registers_the_session_start_index_build_too() {
        let mut v = fresh();
        apply(&mut v, EXE);
        let command = v["hooks"]["SessionStart"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(command.contains("index"), "got {command}");
        // --max-age keeps an unconditional session-start hook cheap.
        assert!(command.contains("--max-age"), "got {command}");
    }

    #[test]
    fn uninstall_removes_both_hooks() {
        let mut v = fresh();
        apply(&mut v, EXE);
        remove(&mut v);
        for event in ["UserPromptSubmit", "SessionStart"] {
            let list = v["hooks"][event].as_array().unwrap();
            assert!(list.is_empty(), "{event} still has {list:?}");
        }
    }

    #[test]
    fn an_exe_path_containing_spaces_is_quoted() {
        // Unquoted, the shell would run `C:\Program` with two arguments and
        // the hook would silently never fire.
        let mut v = fresh();
        apply(&mut v, r"C:\Program Files\yp\yp.exe");
        assert_eq!(
            statusline_of(&v),
            r#""C:\Program Files\yp\yp.exe" statusline"#
        );
        assert_eq!(
            v["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"],
            r#""C:\Program Files\yp\yp.exe" hook"#
        );
    }

    #[test]
    fn a_quoted_exe_path_survives_a_reinstall_and_an_uninstall() {
        let exe = r"C:\Program Files\yp\yp.exe";
        let mut v = fresh();
        apply(&mut v, exe);
        let after_first = v.clone();
        apply(&mut v, exe);
        assert_eq!(v, after_first, "reinstall was not a no-op");
        remove(&mut v);
        assert!(v.get("statusLine").is_none(), "got {v}");
    }

    #[test]
    fn a_non_object_settings_file_is_replaced_rather_than_crashing() {
        let mut v = json!([1, 2, 3]);
        let notes = apply(&mut v, EXE);
        assert!(v.is_object());
        assert!(notes.iter().any(|n| n.contains("not a JSON object")));
    }

    #[test]
    fn malformed_hook_sections_are_repaired_rather_than_crashing() {
        let mut v = json!({ "hooks": "nonsense" });
        apply(&mut v, EXE);
        assert_eq!(
            v["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"],
            "/usr/local/bin/yp hook"
        );
    }
}
