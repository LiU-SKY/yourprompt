//! The sidecar store: how a score gets from the hook to the status line
//! without passing through the model's context.
//!
//! Claude Code injects a `UserPromptSubmit` hook's stdout directly into the
//! conversation, so the hook cannot simply print the score -- that would cost
//! tokens on every prompt, which is the exact thing this project exists to
//! avoid. Instead the hook writes the score to a file keyed by session id, and
//! the status line command (whose output never reaches the model) reads it
//! back.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use yp_core::{Kind, Score};

/// Bump when the on-disk shape changes in a way older readers cannot handle.
///
/// Still 1 after `kind` and `language` were added to [`Entry`]: a sidecar
/// written before them loads with no kind, and older readers ignore fields
/// they do not know.
pub const SIDECAR_VERSION: u32 = 1;

/// A move smaller than this is shown as no move at all.
///
/// Scores are printed to one decimal, so a smaller difference is one the user
/// cannot see. The status line draws it as "·" and `yp explain` calls it the
/// same score; before they shared this, explain said "down 0.0" beside a "·".
pub const UNCHANGED: f64 = 0.05;

/// How many past scores to keep, for trend display.
const HISTORY_LIMIT: usize = 20;

/// Longest prompt kept verbatim for `yp explain`.
///
/// The status line re-reads this file constantly, so an enormous pasted
/// snippet must not make it enormous too. Anything longer is stored truncated
/// and flagged, and `explain` says so rather than quietly explaining a
/// different prompt than the one that was scored.
const PROMPT_LIMIT: usize = 16 * 1024;

/// Sidecars older than this are removed. Sessions are ephemeral; there is no
/// reason to accumulate them forever in the user's home directory.
const TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// One scored prompt, reduced to what the status line actually renders.
///
/// The full [`Score`] with all its component detail is far more than a status
/// line needs, and writing it on every keystroke would be wasteful. `yp score`
/// recomputes the breakdown on demand instead.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    pub total: f64,
    pub grade: String,
    /// True when axis A was unavailable and the rest were rescaled.
    pub renormalized: bool,
    /// Length of the prompt in characters, for debugging and for the
    /// eventual session report.
    pub prompt_chars: usize,
    /// What the prompt was. Commands and follow-ups are never recorded, so
    /// this is a task, or an unsupported prompt the status line has to say
    /// it did not grade.
    ///
    /// `None` on an entry written before kinds existed. Those cannot be
    /// assumed to be tasks: v0.1.0 recorded "continue" and "/score" as well,
    /// and after an upgrade the status line went on showing a "/score" graded
    /// 31.2 F, and the next task's trend was measured from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    /// The language an unsupported prompt looked like, when that could be
    /// told.
    #[serde(default)]
    pub language: Option<String>,
}

impl Entry {
    pub fn from_score(score: &Score, prompt: &str) -> Self {
        Self {
            total: score.display_total(),
            grade: score.grade.to_string(),
            renormalized: score.renormalized,
            prompt_chars: prompt.chars().count(),
            kind: Some(score.kind),
            language: score.language.map(str::to_string),
        }
    }

    /// True when `total` and `grade` describe the prompt. They are stored
    /// for every entry, but only a task's are meant to be shown, and an entry
    /// of unknown kind is not known to be one.
    pub fn is_graded(&self) -> bool {
        self.kind.is_some_and(Kind::is_graded)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sidecar {
    pub version: u32,
    pub session_id: String,
    /// Unix seconds. Only used for expiry, never for scoring -- the score
    /// itself must not depend on a clock.
    pub updated_unix: u64,
    #[serde(default)]
    pub cwd: String,
    pub latest: Entry,
    /// The prompt behind `latest`, kept so `yp explain` can show the full
    /// breakdown without the user retyping it. Local only -- it never leaves
    /// this file, and never enters the model's context.
    #[serde(default)]
    pub latest_prompt: String,
    /// True when `latest_prompt` was cut short by `PROMPT_LIMIT`.
    #[serde(default)]
    pub prompt_truncated: bool,
    /// True when what was typed ran past what the scorer reads of it, so
    /// `latest` describes only its start: see
    /// [`yp_core::prompt::MAX_INSTRUCTION_BYTES`]. False in a sidecar written
    /// before the scorer had a limit, which read every prompt whole.
    #[serde(default)]
    pub prompt_capped: bool,
    /// The same for what was pasted into the prompt, of which the scorer
    /// reads [`yp_core::prompt::MAX_ATTACHED_BYTES`]. A long paste is the
    /// common case: everything typed around it is still read.
    #[serde(default)]
    pub paste_capped: bool,
    /// Most recent first.
    #[serde(default)]
    pub history: Vec<Entry>,
}

impl Sidecar {
    /// The previous graded score in this session, if there is one.
    ///
    /// An ungraded entry has no number worth comparing against, so the trend
    /// arrow and `yp explain` look past it to the last task. So do entries
    /// from before kinds existed, which may be follow-ups.
    pub fn previous(&self) -> Option<&Entry> {
        self.history.iter().find(|entry| entry.is_graded())
    }

    /// What the latest prompt was, and its language when it is unsupported.
    ///
    /// A sidecar written before kinds existed stores neither, so its prompt
    /// is classified again, which needs no index: it is the check the hook
    /// makes before it loads one.
    pub fn latest_kind(&self) -> (Kind, Option<String>) {
        match self.latest.kind {
            Some(kind) => (kind, self.latest.language.clone()),
            None => {
                let (kind, language) = yp_core::kind_of(&self.latest_prompt);
                (kind, language.map(str::to_string))
            }
        }
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The directory Claude Code keeps its configuration in.
///
/// Honours `CLAUDE_CONFIG_DIR` the way Claude Code itself does, then falls
/// back to `~/.claude`. `YP_STATE_DIR` overrides everything and exists so
/// tests never touch the real home directory.
pub fn state_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("YP_STATE_DIR") {
        return Some(PathBuf::from(dir));
    }
    let base = if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        PathBuf::from(dir)
    } else {
        home_dir()?.join(".claude")
    };
    Some(base.join("yourprompt"))
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

fn sessions_dir() -> Option<PathBuf> {
    state_dir().map(|d| d.join("sessions"))
}

/// Session ids come from Claude Code, but they end up in a file path, so they
/// are sanitised rather than trusted.
fn safe_id(session_id: &str) -> String {
    let cleaned: String = session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(128)
        .collect();
    if cleaned.is_empty() {
        "unknown".to_string()
    } else {
        cleaned
    }
}

pub fn sidecar_path(session_id: &str) -> Option<PathBuf> {
    sessions_dir().map(|d| d.join(format!("{}.json", safe_id(session_id))))
}

pub fn load(session_id: &str) -> Option<Sidecar> {
    let path = sidecar_path(session_id)?;
    let text = fs::read_to_string(path).ok()?;
    let sidecar: Sidecar = serde_json::from_str(&text).ok()?;
    if sidecar.version != SIDECAR_VERSION {
        return None;
    }
    Some(sidecar)
}

/// Record a score, rolling the previous latest into the history.
pub fn record(session_id: &str, cwd: &str, prompt: &str, entry: Entry) -> io::Result<()> {
    let Some(dir) = sessions_dir() else {
        return Err(io::Error::other("no state directory available"));
    };
    let existing = load(session_id);
    let first_write = existing.is_none();

    let mut history = Vec::new();
    if let Some(previous) = existing {
        let (kind, language) = previous.latest_kind();
        let mut latest = previous.latest;
        if latest.kind.is_none() {
            latest.kind = Some(kind);
            latest.language = language;
        }
        // A command or follow-up can only be here from a sidecar written
        // before kinds existed. This version would never have recorded it,
        // so it does not enter the history either.
        if !matches!(kind, Kind::Command | Kind::FollowUp) {
            history.push(latest);
        }
        history.extend(previous.history);
        history.truncate(HISTORY_LIMIT);
    }

    let (stored_prompt, truncated) = truncate_on_char_boundary(prompt, PROMPT_LIMIT);
    let (prompt_capped, paste_capped) = capped(prompt);
    let sidecar = Sidecar {
        version: SIDECAR_VERSION,
        session_id: session_id.to_string(),
        updated_unix: now_unix(),
        cwd: cwd.to_string(),
        latest: entry,
        latest_prompt: stored_prompt.to_string(),
        prompt_truncated: truncated,
        prompt_capped,
        paste_capped,
        history,
    };

    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", safe_id(session_id)));
    write_atomic(&path, &serde_json::to_vec(&sidecar)?)?;

    // Pruning walks the whole directory, so it runs only when a session is
    // first seen rather than on every prompt.
    if first_write {
        let _ = prune_expired(&dir);
    }
    Ok(())
}

/// Whether the scorer read only the start of what was typed in `prompt`, and
/// whether it read only the start of what was pasted into it, as
/// [`yp_core::prompt::Parts::capped`] decides.
fn capped(prompt: &str) -> (bool, bool) {
    use yp_core::prompt::{split, MAX_ATTACHED_BYTES, MAX_INSTRUCTION_BYTES};
    // Neither part is longer than the whole, so a prompt within both limits
    // is never split to find out, which costs a pass over the text.
    if prompt.len() <= MAX_INSTRUCTION_BYTES.min(MAX_ATTACHED_BYTES) {
        return (false, false);
    }
    let parts = split(prompt);
    let pasted: usize = parts.attachments.iter().map(|a| a.len()).sum();
    (
        parts.instruction().len() > MAX_INSTRUCTION_BYTES,
        pasted > MAX_ATTACHED_BYTES,
    )
}

/// Cut a string to at most `limit` bytes without splitting a character.
fn truncate_on_char_boundary(text: &str, limit: usize) -> (&str, bool) {
    if text.len() <= limit {
        return (text, false);
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], true)
}

/// The sidecar most recently written from `cwd`, or the most recent overall.
///
/// A guess, for when `yp explain` is not told which session it is in: run
/// from a terminal, or under a Claude Code older than 2.1.132, which does not
/// set `CLAUDE_CODE_SESSION_ID`. Preferring a matching working directory
/// makes the guess right whenever the user has several sessions open in
/// different projects, but not two in one.
pub fn most_recent(cwd: &str) -> Option<Sidecar> {
    let dir = sessions_dir()?;
    let mut best: Option<Sidecar> = None;
    for entry in fs::read_dir(dir).ok()? {
        let Ok(entry) = entry else { continue };
        if entry.path().extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok(sidecar) = serde_json::from_str::<Sidecar>(&text) else {
            continue;
        };
        if sidecar.version != SIDECAR_VERSION {
            continue;
        }
        let better = match &best {
            None => true,
            Some(current) => {
                let candidate_matches = !cwd.is_empty() && sidecar.cwd == cwd;
                let current_matches = !cwd.is_empty() && current.cwd == cwd;
                match (candidate_matches, current_matches) {
                    (true, false) => true,
                    (false, true) => false,
                    _ => sidecar.updated_unix > current.updated_unix,
                }
            }
        };
        if better {
            best = Some(sidecar);
        }
    }
    best
}

/// Write via a temporary file and a rename, so a reader never observes a
/// half-written sidecar. `fs::rename` replaces the destination on both Unix
/// and Windows.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    fs::write(&tmp, bytes)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Delete sidecars that have not been touched within [`TTL`].
pub fn prune_expired(dir: &Path) -> io::Result<usize> {
    let mut removed = 0;
    let now = SystemTime::now();
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if now.duration_since(modified).is_ok_and(|age| age > TTL) && fs::remove_file(&path).is_ok()
        {
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Point the store at a scratch directory for the duration of a test.
    ///
    /// `YP_STATE_DIR` is process-global, so these tests must not run in
    /// parallel with each other; they share one lock.
    struct Scratch {
        dir: PathBuf,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    impl Scratch {
        fn new(name: &str) -> Self {
            let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let dir = std::env::temp_dir().join(format!("yp-test-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            std::env::set_var("YP_STATE_DIR", &dir);
            Scratch { dir, _guard: guard }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            std::env::remove_var("YP_STATE_DIR");
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn entry(total: f64) -> Entry {
        Entry {
            total,
            grade: "B".into(),
            renormalized: true,
            prompt_chars: 10,
            kind: Some(Kind::Task),
            language: None,
        }
    }

    /// A v0.1.0 sidecar: no kinds anywhere, and `latest_prompt` as given.
    fn write_v010_sidecar(id: &str, latest_prompt: &str) {
        let dir = sessions_dir().unwrap();
        fs::create_dir_all(&dir).unwrap();
        let old = serde_json::json!({
            "version": 1, "session_id": id, "updated_unix": 1, "cwd": "",
            "latest": {"total": 31.2, "grade": "F", "renormalized": true, "prompt_chars": 6},
            "latest_prompt": latest_prompt, "prompt_truncated": false,
            "history": [
                {"total": 25.0, "grade": "F", "renormalized": true, "prompt_chars": 8},
                {"total": 732.9, "grade": "A-", "renormalized": true, "prompt_chars": 140}
            ]
        });
        fs::write(dir.join(format!("{id}.json")), old.to_string()).unwrap();
    }

    #[test]
    fn a_sidecar_written_before_kinds_existed_still_loads() {
        let _s = Scratch::new("pre-kind");
        write_v010_sidecar("old", "fix it");

        let loaded = load("old").expect("an old sidecar must still load");
        assert_eq!(loaded.latest.kind, None);
        assert_eq!(loaded.latest.language, None);
        // Its prompt is classified again rather than assumed to be a task.
        assert_eq!(loaded.latest_kind(), (Kind::Task, None));
        assert_eq!(load_with_latest("/score").latest_kind().0, Kind::Command);
        assert_eq!(load_with_latest("continue").latest_kind().0, Kind::FollowUp);
        // v0.1.0 recorded "continue" (25.0) between the task and "/score",
        // and nothing in its history says which entry was which.
        assert!(loaded.previous().is_none());
    }

    fn load_with_latest(prompt: &str) -> Sidecar {
        write_v010_sidecar("probe", prompt);
        load("probe").unwrap()
    }

    #[test]
    fn upgrading_over_a_v010_sidecar_keeps_its_slash_command_out_of_the_history() {
        // The reported sequence: a 732.9 task, "continue" (25.0) and "/score"
        // (31.2), all recorded by v0.1.0, then a task scored by this version.
        let _s = Scratch::new("pre-kind-upgrade");
        write_v010_sidecar("old", "/score");
        record("old", "", "fix the bug in statusline.rs", entry(661.7)).unwrap();

        let loaded = load("old").unwrap();
        assert_eq!(loaded.latest.kind, Some(Kind::Task));
        assert_eq!(loaded.history.len(), 2, "{:?}", loaded.history);
        // The legacy entries stay of unknown kind on disk, rather than being
        // written back as tasks.
        assert!(loaded.history.iter().all(|e| e.kind.is_none()));
        let text = fs::read_to_string(sidecar_path("old").unwrap()).unwrap();
        assert_eq!(text.matches("\"kind\"").count(), 1, "{text}");
        // So the trend is not measured from the 31.2 "/score".
        assert!(loaded.previous().is_none());
    }

    #[test]
    fn upgrading_over_a_v010_task_keeps_it_with_its_kind() {
        let _s = Scratch::new("pre-kind-task");
        write_v010_sidecar("old", "fix verify_token in src/auth.rs");
        record("old", "", "fix the parser in src/cli.rs", entry(500.0)).unwrap();

        let loaded = load("old").unwrap();
        assert_eq!(loaded.history[0].kind, Some(Kind::Task));
        assert_eq!(loaded.previous().unwrap().total, 31.2);
    }

    #[test]
    fn the_previous_score_skips_an_ungraded_prompt() {
        let _s = Scratch::new("skip-ungraded");
        record("s", "", "fix the parser", entry(300.0)).unwrap();
        let mut unsupported = entry(149.1);
        unsupported.kind = Some(Kind::Unsupported);
        unsupported.language = Some("Spanish".into());
        record("s", "", "arregla el bug", unsupported).unwrap();
        record("s", "", "fix the parser in src/cli.rs", entry(500.0)).unwrap();

        let loaded = load("s").unwrap();
        assert_eq!(loaded.history[0].kind, Some(Kind::Unsupported));
        assert_eq!(loaded.previous().unwrap().total, 300.0);
    }

    #[test]
    fn records_and_reads_back_a_score() {
        let _s = Scratch::new("roundtrip");
        record("abc123", "/tmp/project", "fix it", entry(742.3)).unwrap();
        let loaded = load("abc123").expect("sidecar should exist");
        assert_eq!(loaded.latest.total, 742.3);
        assert_eq!(loaded.session_id, "abc123");
        assert_eq!(loaded.cwd, "/tmp/project");
        assert!(loaded.history.is_empty());
    }

    #[test]
    fn a_prompt_longer_than_the_scorer_reads_is_marked() {
        // `yp explain` said a 204,828-character prompt's score "was computed
        // on the whole prompt", but the scorer reads 64 KB of it.
        let _s = Scratch::new("capped");
        let capped = |id: &str| {
            let loaded = load(id).unwrap();
            (loaded.prompt_capped, loaded.paste_capped)
        };
        let head = "fix verify_token in src/auth/login.rs ";
        let long = head.to_string() + &"x ".repeat(110_000);
        record("long", "", &long, entry(221.6)).unwrap();
        assert!(load("long").unwrap().prompt_truncated);
        assert_eq!(capped("long"), (true, false));

        // Longer than 64 KB, but most of it pasted, which is cut out and
        // read up to a limit of its own: all of it was scored.
        let fence = |lines: usize| format!("```\n{}\n```", "let x = 1;\n".repeat(lines));
        let pasted = format!("{head}{}\n{}", "y ".repeat(25_000), fence(2_000));
        assert!(pasted.len() > yp_core::prompt::MAX_INSTRUCTION_BYTES);
        record("pasted", "", &pasted, entry(500.0)).unwrap();
        assert_eq!(capped("pasted"), (false, false));

        // A paste past its own limit, with the request typed after it: the
        // request was read whole.
        let log = format!("{}\n{head}", fence(8_000));
        record("log", "", &log, entry(400.0)).unwrap();
        assert_eq!(capped("log"), (false, true));
        let both = format!("{log}{}", "z ".repeat(40_000));
        record("both", "", &both, entry(200.0)).unwrap();
        assert_eq!(capped("both"), (true, true));

        record("short", "", head, entry(300.0)).unwrap();
        assert_eq!(capped("short"), (false, false));
    }

    #[test]
    fn missing_session_reads_as_none() {
        let _s = Scratch::new("missing");
        assert!(load("never-written").is_none());
    }

    #[test]
    fn successive_scores_roll_into_history_newest_first() {
        let _s = Scratch::new("history");
        for total in [100.0, 200.0, 300.0] {
            record("s", "", "p", entry(total)).unwrap();
        }
        let loaded = load("s").unwrap();
        assert_eq!(loaded.latest.total, 300.0);
        assert_eq!(loaded.previous().unwrap().total, 200.0);
        assert_eq!(loaded.history[1].total, 100.0);
    }

    #[test]
    fn history_is_bounded() {
        let _s = Scratch::new("bounded");
        for i in 0..(HISTORY_LIMIT + 10) {
            record("s", "", "p", entry(i as f64)).unwrap();
        }
        assert_eq!(load("s").unwrap().history.len(), HISTORY_LIMIT);
    }

    #[test]
    fn session_ids_cannot_escape_the_sessions_directory() {
        let _s = Scratch::new("traversal");
        let path = sidecar_path("../../etc/passwd").unwrap();
        assert_eq!(path.file_name().unwrap(), "etcpasswd.json");
        assert!(!path.to_string_lossy().contains(".."));
    }

    #[test]
    fn an_empty_session_id_still_produces_a_usable_path() {
        let _s = Scratch::new("emptyid");
        assert_eq!(
            sidecar_path("").unwrap().file_name().unwrap(),
            "unknown.json"
        );
    }

    #[test]
    fn a_sidecar_from_a_future_version_is_ignored_rather_than_misread() {
        let _s = Scratch::new("version");
        record("s", "", "p", entry(500.0)).unwrap();
        let path = sidecar_path("s").unwrap();
        let text = fs::read_to_string(&path).unwrap();
        fs::write(&path, text.replace("\"version\":1", "\"version\":999")).unwrap();
        assert!(load("s").is_none());
    }

    #[test]
    fn corrupt_json_reads_as_none_instead_of_panicking() {
        let _s = Scratch::new("corrupt");
        record("s", "", "p", entry(500.0)).unwrap();
        fs::write(sidecar_path("s").unwrap(), b"{not json").unwrap();
        assert!(load("s").is_none());
    }

    #[test]
    fn pruning_removes_only_expired_sidecars() {
        let _s = Scratch::new("prune");
        record("fresh", "", "p", entry(500.0)).unwrap();
        let dir = sessions_dir().unwrap();

        // A file whose mtime is far in the past stands in for an old session.
        let stale = dir.join("stale.json");
        fs::write(&stale, b"{}").unwrap();
        let old = SystemTime::now() - TTL - Duration::from_secs(60);
        filetime_set(&stale, old);

        let removed = prune_expired(&dir).unwrap();
        assert_eq!(removed, 1);
        assert!(!stale.exists());
        assert!(load("fresh").is_some());
    }

    /// Set a file's modification time without pulling in a dependency.
    fn filetime_set(path: &Path, time: SystemTime) {
        let file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(time).unwrap();
    }
}
