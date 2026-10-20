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
use std::sync::atomic::{AtomicUsize, Ordering};

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

/// Most threads [`read_sources`] reads on.
///
/// Measured on files nobody had read before, whose first read cost 3.9 to
/// 4.9 ms apiece on one thread. Of 3,076 such files, 2 threads read their
/// share 1.9 times as fast as one, 4 threads 3.1 times, 8 threads 4.8, 16
/// threads 9.6 and 28 threads 12. Of small XML files, 16 threads read 8.1 to
/// 9.2 times as fast as one, and 28 threads hardly faster than 16.
const MAX_READERS: usize = 16;

/// The contents of every file in `paths` that is UTF-8, each named by its
/// path below `root` with forward slashes, in the order of `paths`.
///
/// Read on several threads, because the first read of a file can cost far
/// more than its size. Where an antivirus scans each file as it is opened, as
/// Windows Defender does by default, that scan is most of the cost: indexing
/// a folder of 990 to 1,231 small files nobody had opened took 4.5 to 4.9 s
/// with one thread reading and 0.48 to 0.54 s like this, and 61 to 102 ms
/// either way once they had been read. Read one at a time, the first index
/// of a 5,934-file repository took 40 to 71 s, and 3.2 to 8.5 s once its
/// files had been read, 3.0 s of it extracting and sorting 647,056 terms.
/// The order, and so the index, is the same however the reads are shared
/// out.
pub fn read_sources(root: &Path, paths: &[PathBuf]) -> Vec<(String, String)> {
    let readers = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .clamp(1, MAX_READERS)
        .min(paths.len().max(1));
    let next = AtomicUsize::new(0);
    let read = || {
        let mut got = Vec::new();
        loop {
            let i = next.fetch_add(1, Ordering::Relaxed);
            let Some(path) = paths.get(i) else {
                return got;
            };
            // Non-UTF-8 files are skipped rather than lossily decoded: a
            // mis-decoded binary would inject thousands of junk terms.
            if let Ok(text) = std::fs::read_to_string(path) {
                let name = path
                    .strip_prefix(root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/");
                got.push((i, name, text));
            }
        }
    };
    let mut found = std::thread::scope(|scope| {
        let workers: Vec<_> = (1..readers).map(|_| scope.spawn(read)).collect();
        let mut found = read();
        for worker in workers {
            match worker.join() {
                Ok(got) => found.extend(got),
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
        found
    });
    found.sort_unstable_by_key(|&(i, ..)| i);
    found
        .into_iter()
        .map(|(_, name, text)| (name, text))
        .collect()
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
    fn files_read_on_several_threads_come_back_in_order() {
        let root = std::env::temp_dir().join(format!("yp-read-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src/deep")).unwrap();
        let mut paths = Vec::new();
        let mut expected = Vec::new();
        for i in 0..200 {
            let name = if i % 3 == 0 {
                format!("src/deep/f{i}.rs")
            } else {
                format!("f{i}.rs")
            };
            let path = root.join(&name);
            if i % 7 == 0 {
                // Not UTF-8, so skipped, wherever it falls.
                std::fs::write(&path, [0xff, 0xfe, 0x00, i as u8]).unwrap();
            } else {
                let text = format!("fn f{i}() {{}}\n");
                std::fs::write(&path, &text).unwrap();
                expected.push((name, text));
            }
            paths.push(path);
        }
        // One that disappeared between the walk and the read.
        paths.insert(50, root.join("gone.rs"));

        let read = read_sources(&root, &paths);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(read, expected);
        assert!(read_sources(&root, &[]).is_empty());
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
