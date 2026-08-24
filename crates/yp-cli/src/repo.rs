//! Finding, building and loading the repository index.
//!
//! The adapter between `yp-index` (which knows how to read a repository) and
//! `yp-core` (which knows what to do with the statistics). Kept in the CLI so
//! that `yp-core` stays free of file access.

use std::path::{Path, PathBuf};

use yp_core::{Corpus, TermFacts};
use yp_index::RepoIndex;

use crate::session;

/// A loaded index, presented to the scorer.
pub struct IndexCorpus(RepoIndex);

impl Corpus for IndexCorpus {
    fn lookup(&self, term: &str) -> Option<TermFacts> {
        self.0.lookup(term).map(|s| TermFacts {
            df: s.df,
            cf: s.cf,
            def: s.def,
        })
    }

    fn documents(&self) -> usize {
        self.0.files()
    }

    fn total_terms(&self) -> u64 {
        self.0.total_terms()
    }
}

/// The repository root containing `start`.
///
/// Walks up looking for a `.git` directory (or file, for worktrees and
/// submodules). Falls back to `start` itself, so the index still works in a
/// directory that is not a repository at all.
pub fn repo_root(start: &Path) -> PathBuf {
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

pub fn index_dir() -> Option<PathBuf> {
    session::state_dir().map(|d| d.join("index"))
}

pub fn index_path(root: &Path) -> Option<PathBuf> {
    index_dir().map(|d| d.join(yp_index::index_file_name(root)))
}

/// Load the index for the repository containing `cwd`, if one has been built.
///
/// Returns `None` rather than building: this is called from the hook, which
/// runs on every prompt and must never block on indexing a large repository.
/// A missing index costs the grounding axis, not the user's time.
pub fn load_for(cwd: &Path) -> Option<IndexCorpus> {
    let root = repo_root(cwd);
    let path = index_path(&root)?;
    let index = RepoIndex::load(&path)?;
    if index.is_empty() {
        return None;
    }
    Some(IndexCorpus(index))
}

/// Build and save the index for the repository containing `cwd`.
pub fn build_for(cwd: &Path) -> std::io::Result<(PathBuf, usize, usize)> {
    let root = repo_root(cwd);
    let Some(path) = index_path(&root) else {
        return Err(std::io::Error::other("no state directory available"));
    };
    let index = RepoIndex::build(&root);
    index.save(&path)?;
    Ok((root, index.files(), index.distinct_terms()))
}

/// How long ago the index for `cwd` was written.
pub fn age(cwd: &Path) -> Option<std::time::Duration> {
    let path = index_path(&repo_root(cwd))?;
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
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

        assert_eq!(repo_root(&nested), base.join("outer"));

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
        assert!(
            dir.ancestors().any(|a| a == root),
            "{} is not an ancestor of {}",
            root.display(),
            dir.display()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn index_paths_differ_per_repository() {
        let a = yp_index::index_file_name(Path::new("/one"));
        let b = yp_index::index_file_name(Path::new("/two"));
        assert_ne!(a, b);
    }
}
