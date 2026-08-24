//! Deciding which files are part of "the repository".
//!
//! In a git repository we ask git, which gets exact `.gitignore` semantics --
//! including nested ignore files, negations and the global excludes file --
//! for free and faster than we could walk the tree ourselves. Outside one, we
//! fall back to a bounded manual walk with a built-in skip list.
//!
//! Deliberately no `ignore`/`globset` dependency: it would pull the regex
//! engine into a binary whose whole pitch is being small and starting fast.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Directories never worth indexing, used only in the non-git fallback.
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    "vendor",
    ".venv",
    "venv",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".next",
    ".nuxt",
    ".gradle",
    ".idea",
    ".vscode",
    "Pods",
    "DerivedData",
];

/// Extensions that are certainly not source text.
const SKIP_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "ico", "bmp", "tiff", "svg", "pdf", "zip", "gz", "tar",
    "bz2", "xz", "7z", "rar", "jar", "war", "class", "exe", "dll", "so", "dylib", "a", "o", "obj",
    "pdb", "bin", "dat", "db", "sqlite", "mp3", "mp4", "wav", "mov", "avi", "webm", "ttf", "otf",
    "woff", "woff2", "eot", "lock", "min.js", "map",
];

/// Files above this size are skipped: generated bundles and vendored blobs
/// would swamp the term statistics with noise.
pub const MAX_FILE_BYTES: u64 = 512 * 1024;

/// Upper bound on how many files one index covers, so a monorepo cannot make
/// indexing take unbounded time.
pub const MAX_FILES: usize = 20_000;

fn is_skippable(path: &Path) -> bool {
    let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase());
    let Some(name) = name else { return true };
    if let Some((_, ext)) = name.rsplit_once('.') {
        if SKIP_EXTS.contains(&ext) {
            return true;
        }
    }
    // Minified bundles are technically text but are all noise.
    name.ends_with(".min.js") || name.ends_with(".min.css")
}

/// Ask git for the tracked and untracked-but-not-ignored files.
fn git_files(root: &Path) -> Option<Vec<PathBuf>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Some(
        text.split('\0')
            .filter(|s| !s.is_empty())
            .map(|s| root.join(s))
            .collect(),
    )
}

/// Walk the tree ourselves, skipping the usual suspects.
fn manual_walk(root: &Path) -> Vec<PathBuf> {
    let skip: HashSet<&str> = SKIP_DIRS.iter().copied().collect();
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        if found.len() >= MAX_FILES {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            // Symlinks are not followed: a link pointing up the tree would
            // send the walk in circles.
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if skip.contains(name.as_ref()) || name.starts_with('.') {
                    continue;
                }
                stack.push(path);
            } else if kind.is_file() {
                found.push(path);
            }
        }
    }
    found
}

/// Every file worth indexing under `root`.
pub fn source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = git_files(root).unwrap_or_else(|| manual_walk(root));
    files.retain(|p| {
        if is_skippable(p) {
            return false;
        }
        std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() <= MAX_FILE_BYTES)
    });
    files.truncate(MAX_FILES);
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_and_minified_files_are_skipped() {
        assert!(is_skippable(Path::new("logo.png")));
        assert!(is_skippable(Path::new("app.min.js")));
        assert!(is_skippable(Path::new("Cargo.lock")));
        assert!(is_skippable(Path::new("target/debug/yp.exe")));
    }

    #[test]
    fn source_files_are_not_skipped() {
        assert!(!is_skippable(Path::new("src/main.rs")));
        assert!(!is_skippable(Path::new("app.js")));
        assert!(!is_skippable(Path::new("README.md")));
        assert!(!is_skippable(Path::new("Makefile")));
    }

    #[test]
    fn indexes_this_repository_through_git() {
        // This crate lives in a git repository, so the git path is exercised.
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let files = source_files(root);
        assert!(!files.is_empty(), "found no files under {}", root.display());
        assert!(
            files.iter().any(|p| p.ends_with("Cargo.toml")),
            "expected to find the workspace manifest"
        );
        assert!(
            !files.iter().any(|p| p.to_string_lossy().contains("target")),
            "build output must be excluded"
        );
    }

    #[test]
    fn the_manual_walk_skips_noise_directories() {
        let root = std::env::temp_dir().join(format!("yp-walk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("node_modules/pkg/index.js"), "x").unwrap();
        std::fs::write(root.join(".git/config"), "x").unwrap();

        let found = manual_walk(&root);
        let names: Vec<String> = found.iter().map(|p| p.display().to_string()).collect();
        assert!(names.iter().any(|n| n.ends_with("main.rs")), "{names:?}");
        assert!(
            !names.iter().any(|n| n.contains("node_modules")),
            "{names:?}"
        );
        assert!(!names.iter().any(|n| n.contains(".git")), "{names:?}");

        let _ = std::fs::remove_dir_all(&root);
    }
}
