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
use yp_core::Score;

/// Bump when the on-disk shape changes in a way older readers cannot handle.
pub const SIDECAR_VERSION: u32 = 1;

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
}

impl Entry {
    pub fn from_score(score: &Score, prompt: &str) -> Self {
        Self {
            total: score.display_total(),
            grade: score.grade.to_string(),
            renormalized: score.renormalized,
            prompt_chars: prompt.chars().count(),
        }
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
    /// Most recent first.
    #[serde(default)]
    pub history: Vec<Entry>,
}

impl Sidecar {
    /// The previous score in this session, if there is one.
    pub fn previous(&self) -> Option<&Entry> {
        self.history.first()
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
        history.push(previous.latest);
        history.extend(previous.history);
        history.truncate(HISTORY_LIMIT);
    }

    let (stored_prompt, truncated) = truncate_on_char_boundary(prompt, PROMPT_LIMIT);
    let sidecar = Sidecar {
        version: SIDECAR_VERSION,
        session_id: session_id.to_string(),
        updated_unix: now_unix(),
        cwd: cwd.to_string(),
        latest: entry,
        latest_prompt: stored_prompt.to_string(),
        prompt_truncated: truncated,
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
/// Slash commands do not receive a session id, so `yp explain` has to guess.
/// Preferring a matching working directory makes the guess right whenever the
/// user has several sessions open in different projects.
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
        }
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
