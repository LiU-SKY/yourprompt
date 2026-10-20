//! Finding, building and loading the repository index.
//!
//! The adapter between `yp-index` (which knows how to read a repository) and
//! `yp-core` (which knows what to do with the statistics). Kept in the CLI so
//! that `yp-core` stays free of file access.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use yp_core::{Corpus, TermFacts};
use yp_index::RepoIndex;

use crate::session;

/// A loaded index, presented to the scorer.
pub struct IndexCorpus {
    index: RepoIndex,
    /// When the index file was written, if the filesystem says.
    written: Option<SystemTime>,
}

impl IndexCorpus {
    /// When the index file was written. `yp explain` compares it with when
    /// the prompt was scored, to tell whether the status line's number came
    /// from an older index.
    pub fn written(&self) -> Option<SystemTime> {
        self.written
    }
}

impl Corpus for IndexCorpus {
    fn lookup(&self, term: &str) -> Option<TermFacts> {
        self.index.lookup(term).map(|s| TermFacts {
            df: s.df,
            cf: s.cf,
            def: s.def,
        })
    }

    fn documents(&self) -> usize {
        self.index.files()
    }

    fn total_terms(&self) -> u64 {
        self.index.total_terms()
    }

    fn paths_ending_in(&self, tail: &str) -> Option<TermFacts> {
        self.index.paths_ending_in(tail).map(|s| TermFacts {
            df: s.df,
            cf: s.cf,
            def: s.def,
        })
    }
}

/// One spelling for one directory: absolute, with symlinks resolved, and on
/// Windows without the `\\?\` prefix and with an upper-case drive letter.
///
/// The index file is named after a hash of its root, so every spelling used
/// to get an index of its own. A repository indexed as
/// `C:/Users/.../repo`, the way Git Bash types it, was missed by a hook whose
/// payload said `C:\Users\...\repo`, and a task that scored 529.8 grounded
/// came out as an ungrounded 356.4. `c:\...` and a trailing backslash missed
/// the same way.
pub fn canonical(path: &Path) -> PathBuf {
    let resolved = std::fs::canonicalize(path)
        // A directory that is gone still gets one spelling, made absolute
        // without touching the disk.
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf());
    plain(resolved)
}

/// `path` without the Windows verbatim prefix `canonicalize` adds, which
/// nothing else writes, and with its drive letter in upper case.
fn plain(path: PathBuf) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path;
    };
    let mut text = if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        text.strip_prefix(r"\\?\").unwrap_or(text).to_string()
    };
    if text.as_bytes().get(1) == Some(&b':') {
        text[..1].make_ascii_uppercase();
    }
    // `absolute` keeps a trailing separator that `canonicalize` drops; a
    // root keeps the one that makes it a root.
    let is_root = |t: &str| t == "/" || (t.len() == 3 && t.as_bytes()[1] == b':');
    while text.ends_with(['/', '\\']) && !is_root(&text) {
        text.pop();
    }
    PathBuf::from(text)
}

/// The home directory, spelt as [`canonical`] spells it.
fn home() -> Option<PathBuf> {
    session::home_dir().map(|h| canonical(&h))
}

/// The repository root containing `start`.
///
/// Walks up looking for a `.git` directory (or file, for worktrees and
/// submodules). Falls back to `start` itself, so the index still works in a
/// directory that is not a repository at all.
///
/// The walk stops at the home directory. A home directory under version
/// control (dotfiles kept in place) otherwise claimed every directory below
/// it that is not a repository itself: a session opened in a scratch folder
/// indexed 19,996 home files, and every such session was then graded
/// against that one index. Home is a root only when it is where you are.
pub fn repo_root(start: &Path) -> PathBuf {
    let start = canonical(start);
    let home = home();
    let mut current = start.as_path();
    loop {
        if home.as_deref() == Some(current) {
            return start;
        }
        if current.join(".git").exists() {
            return current.to_path_buf();
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return start,
        }
    }
}

/// True when `root` is the home directory, which is never indexed: what is
/// under it is everything, not a project.
fn is_home(root: &Path) -> bool {
    home().as_deref() == Some(root)
}

pub fn index_dir() -> Option<PathBuf> {
    session::state_dir().map(|d| d.join("index"))
}

pub fn index_path(root: &Path) -> Option<PathBuf> {
    index_dir().map(|d| d.join(yp_index::index_file_name(root)))
}

/// The root an earlier version would have found for `start`: the same walk
/// over the path exactly as it was spelt.
fn spelt_root(start: &Path) -> PathBuf {
    let mut current = start;
    loop {
        if current.join(".git").exists() {
            return current.to_path_buf();
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return start.to_path_buf(),
        }
    }
}

/// The index file serving the repository containing `cwd`, if one exists.
///
/// Indexes built before roots were made canonical are named after the
/// spelling they were built from, so that name is tried second, when it is
/// the same directory. An upgrade keeps its grounding until the next rebuild
/// writes the canonical name.
fn existing_index(cwd: &Path) -> Option<PathBuf> {
    let root = repo_root(cwd);
    if is_home(&root) {
        return None;
    }
    let path = index_path(&root)?;
    if path.exists() {
        return Some(path);
    }
    // The spellings are compared as text, as the name hashes them. `Path`
    // equality would call `C:\repo\` and `C:/repo` the same path, which
    // they are, and so never try the name the first one was given.
    let spelt = spelt_root(cwd);
    if spelt.as_os_str() == root.as_os_str() || canonical(&spelt) != root {
        return None;
    }
    let earlier = index_path(&spelt)?;
    earlier.exists().then_some(earlier)
}

/// Load an index from an explicit path.
///
/// Used by the benchmark, which keeps one index per benchmarked repository
/// rather than one per working directory.
pub fn load_index_at(path: &Path) -> Option<IndexCorpus> {
    let index = RepoIndex::load(path)?;
    if index.is_empty() {
        return None;
    }
    let written = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    Some(IndexCorpus { index, written })
}

/// Load the index for the repository containing `cwd`, if one has been built.
///
/// Returns `None` rather than building: this is called from the hook, which
/// runs on every prompt and must never block on indexing a large repository.
/// A missing index costs the grounding axis, not the user's time.
pub fn load_for(cwd: &Path) -> Option<IndexCorpus> {
    load_index_at(&existing_index(cwd)?)
}

/// How long the hook waits to find out which index serves its directory.
///
/// Finding out reads the directory and each of its parents. On a network
/// share whose host does not answer, the first of those reads waited out
/// the connection timeout: a cwd of `\\192.0.2.1\share\repo` held the hook
/// for 21 s, past the 5 s Claude Code gives it, so the prompt waited and the
/// status line went stale.
///
/// On a local disk the lookup took at most 5 ms in 560 hooks run 28 at a
/// time, and at most 118 ms in 1,120 more with every core and the disk kept
/// busy besides. A lookup cut short scores the prompt without the index it
/// has, lower for no reason anyone can see, so the deadline is set well clear
/// of that, and still well inside the 5 s.
pub const LOOKUP_DEADLINE: Duration = Duration::from_secs(1);

/// [`load_for`], unless finding the index takes longer than `deadline`.
///
/// `None` when it did, and `Some` of what [`load_for`] would return when it
/// did not. The lookup runs on a thread of its own, which is left behind
/// when it is too slow: a thread blocked on an unreachable share does not
/// hold up the end of the process, and with its cwd on a host that does not
/// answer the hook now ends 1,024 to 1,089 ms after it starts, the prompt
/// scored without an index. The index itself is read on the calling thread
/// once found: it is in the state directory on this machine, and a large
/// one takes the longest to read.
pub fn load_for_within(cwd: &Path, deadline: Duration) -> Option<Option<IndexCorpus>> {
    let cwd = cwd.to_path_buf();
    let found = within(deadline, move || existing_index(&cwd))?;
    Some(found.and_then(|path| load_index_at(&path)))
}

/// What `work` returns, if it returns within `deadline`.
///
/// It runs on a thread of its own, which is abandoned, still running, when
/// it is too slow. `None` too if no thread can be started, since the caller
/// is one that must not wait.
fn within<T: Send + 'static>(
    deadline: Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("index lookup".into())
        .spawn(move || {
            // Nobody may be listening any more; that is fine.
            let _ = sender.send(work());
        })
        .ok()?;
    receiver.recv_timeout(deadline).ok()
}

/// Build and save the index for the repository containing `cwd`.
pub fn build_for(cwd: &Path) -> io::Result<(PathBuf, usize, usize)> {
    let root = repo_root(cwd);
    if is_home(&root) {
        return Err(io::Error::other(format!(
            "{} is your home directory, not a project; run it from a repository",
            root.display()
        )));
    }
    let Some(path) = index_path(&root) else {
        return Err(io::Error::other("no state directory available"));
    };
    let index = RepoIndex::build(&root);
    index.save(&path)?;
    Ok((root, index.files(), index.distinct_terms()))
}

/// How long ago the index for `cwd` was written.
pub fn age(cwd: &Path) -> Option<std::time::Duration> {
    let modified = std::fs::metadata(existing_index(cwd)?)
        .ok()?
        .modified()
        .ok()?;
    std::time::SystemTime::now().duration_since(modified).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_root_of_this_repository() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = repo_root(here);
        assert!(root.join(".git").exists(), "got {}", root.display());
        assert!(root.join("Cargo.toml").exists());
    }

    #[test]
    fn stops_at_the_nearest_enclosing_repository() {
        let base = std::env::temp_dir().join(format!("yp-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let nested = base.join("outer/inner/deep");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(base.join("outer/.git")).unwrap();

        assert_eq!(repo_root(&nested), canonical(&base.join("outer")));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_result_is_always_the_start_or_one_of_its_ancestors() {
        // Cannot assert "no repository found" from a test, because a checkout
        // may sit anywhere -- including inside another repository. What must
        // always hold is that the walk only ever goes upward.
        let dir = std::env::temp_dir().join(format!("yp-anc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let root = repo_root(&dir);
        let start = canonical(&dir);
        assert!(
            start.ancestors().any(|a| a == root),
            "{} is not an ancestor of {}",
            root.display(),
            start.display()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_lookup_that_does_not_answer_in_time_is_given_up_on() {
        // Stands in for a working directory on a share whose host does not
        // answer, where the first read waited 21 s.
        let started = std::time::Instant::now();
        let stalled = within(Duration::from_millis(50), || {
            std::thread::sleep(Duration::from_secs(30));
            PathBuf::from("never")
        });
        assert_eq!(stalled, None);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "waited {:?}",
            started.elapsed()
        );
        assert_eq!(within(Duration::from_secs(10), || 7), Some(7));
    }

    #[test]
    fn a_lookup_in_time_finds_what_an_unbounded_one_finds() {
        let dir = std::env::temp_dir().join(format!("yp-within-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        let root = canonical(&dir);
        let found = {
            let dir = dir.clone();
            within(LOOKUP_DEADLINE, move || repo_root(&dir))
        };
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(found, Some(root));
    }

    #[test]
    fn index_paths_differ_per_repository() {
        let a = yp_index::index_file_name(Path::new("/one"));
        let b = yp_index::index_file_name(Path::new("/two"));
        assert_ne!(a, b);
    }

    #[test]
    fn one_directory_spelt_several_ways_is_one_root() {
        let dir = std::env::temp_dir().join(format!("yp-spelling-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        let text = dir.to_string_lossy().into_owned();
        let mut spellings = vec![text.clone(), format!("{text}/"), format!("{text}/./")];
        if cfg!(windows) {
            // What Git Bash types, a lower-case drive, a trailing backslash.
            spellings.push(text.replace('\\', "/"));
            spellings.push(text[..1].to_ascii_lowercase() + &text[1..]);
            spellings.push(format!("{text}\\"));
        }
        let roots: Vec<PathBuf> = spellings.iter().map(|s| repo_root(Path::new(s))).collect();
        for (spelling, root) in spellings.iter().zip(&roots) {
            assert_eq!(root, &roots[0], "{spelling}");
            let name = index_path(root).map(|p| p.file_name().unwrap().to_owned());
            assert_eq!(
                name,
                index_path(&roots[0]).map(|p| p.file_name().unwrap().to_owned())
            );
        }
        let shown = roots[0].to_string_lossy();
        assert!(!shown.starts_with(r"\\?\"), "got {shown}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_verbatim_prefix_and_a_lower_case_drive_are_dropped() {
        assert_eq!(
            plain(PathBuf::from(r"\\?\C:\Users\me\repo")),
            PathBuf::from(r"C:\Users\me\repo")
        );
        assert_eq!(
            plain(PathBuf::from(r"c:\Users\me\repo\")),
            PathBuf::from(r"C:\Users\me\repo")
        );
        assert_eq!(
            plain(PathBuf::from(r"\\?\UNC\server\share\repo")),
            PathBuf::from(r"\\server\share\repo")
        );
        assert_eq!(plain(PathBuf::from(r"C:\")), PathBuf::from(r"C:\"));
        assert_eq!(plain(PathBuf::from("/")), PathBuf::from("/"));
    }

    /// HOME, and USERPROFILE, which Windows reads, pointed at another
    /// directory until this is dropped, even by a failing assertion. Tests
    /// that touch the environment hold its lock.
    struct Home {
        saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl Home {
        fn at(home: &Path) -> Self {
            static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
            let lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let keys = ["HOME", "USERPROFILE"];
            let saved = keys.map(|key| (key, std::env::var_os(key))).to_vec();
            for key in keys {
                std::env::set_var(key, home);
            }
            Home { saved, _lock: lock }
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            for (key, value) in self.saved.drain(..) {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    #[test]
    fn a_home_directory_under_git_does_not_claim_what_is_below_it() {
        let home = std::env::temp_dir().join(format!("yp-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let scratch = home.join("Downloads/pkg/src");
        let project = home.join("code/project");
        std::fs::create_dir_all(home.join(".git")).unwrap();
        std::fs::create_dir_all(&scratch).unwrap();
        std::fs::create_dir_all(project.join(".git")).unwrap();
        // Every directory here exists, so each is canonical the same way:
        // macOS keeps the temporary directory behind a symlink, and GitHub's
        // Windows runners spell it with an 8.3 name.
        std::fs::create_dir_all(project.join("src")).unwrap();

        {
            let _home = Home::at(&home);
            // Not a repository: indexed on its own, not as part of home.
            assert_eq!(repo_root(&scratch), canonical(&scratch));
            // A repository below home is still found.
            assert_eq!(repo_root(&project.join("src")), canonical(&project));
            // Home itself is a root only when it is where you are, and even
            // then it is not indexed.
            assert_eq!(repo_root(&home), canonical(&home));
            let err = build_for(&home).expect_err("home must not be indexed");
            assert!(err.to_string().contains("home directory"), "{err}");
        }
        let _ = std::fs::remove_dir_all(&home);
    }
}
