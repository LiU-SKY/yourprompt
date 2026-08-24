//! The repository symbol index behind the grounding axis.
//!
//! # Why this file format
//!
//! The index is read by `yp hook`, which runs on every prompt the user types,
//! so loading it has to be nearly free. Parsing a JSON map of fifty thousand
//! terms would cost more than the entire scoring budget. So the index is
//! written as a **sorted, tab-separated text file** and queried by binary
//! search over the raw bytes: no deserialisation at all, a handful of
//! comparisons per lookup, and a prompt only ever has a few dozen referents.
//!
//! Text rather than a packed binary because it costs nothing here and the
//! index stays greppable when someone wants to know why their prompt scored
//! the way it did.
//!
//! # Why building is never on the critical path
//!
//! Indexing a large repository takes seconds. The hook only ever *reads* an
//! index; building happens through `yp index`, which the plugin runs at
//! session start. If no index exists, the grounding axis reports itself
//! unavailable and the other axes are renormalised -- a missing index degrades
//! the score, it never delays a prompt.

pub mod extract;
pub mod walk;

use std::collections::HashMap;
use std::io;
use std::path::Path;

pub use extract::TermStat;

const MAGIC: &str = "#YPIX";
const FORMAT_VERSION: u32 = 2;

/// Longest path kept as a single term. Identifiers are capped much shorter,
/// but a real source path easily exceeds that and is exactly the kind of
/// referent worth resolving.
const MAX_PATH_TERM: usize = 256;

/// A queryable repository index.
pub struct RepoIndex {
    /// The whole file, header included.
    blob: String,
    /// Byte offset of the first record, just past the header line.
    body: usize,
    files: usize,
    total_terms: u64,
    distinct_terms: usize,
}

impl RepoIndex {
    pub fn files(&self) -> usize {
        self.files
    }

    /// Total term occurrences across the repository -- the denominator of the
    /// collection language model the specificity score is built on.
    pub fn total_terms(&self) -> u64 {
        self.total_terms
    }

    pub fn distinct_terms(&self) -> usize {
        self.distinct_terms
    }

    pub fn is_empty(&self) -> bool {
        self.distinct_terms == 0
    }

    /// Look one term up. Case-insensitive: the index stores lowercase.
    pub fn lookup(&self, term: &str) -> Option<TermStat> {
        let needle = term.to_lowercase();
        let body = &self.blob[self.body..];
        if body.is_empty() {
            return None;
        }

        let mut lo = 0usize;
        let mut hi = body.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            // Snap to the start of whichever line `mid` landed inside.
            let line_start = body[..mid].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let line_end = body[line_start..]
                .find('\n')
                .map(|i| line_start + i)
                .unwrap_or(body.len());
            let line = &body[line_start..line_end];
            let key = line.split('\t').next().unwrap_or("");

            match key.cmp(needle.as_str()) {
                std::cmp::Ordering::Equal => return parse_record(line),
                std::cmp::Ordering::Less => {
                    // Everything up to and including this line is too small.
                    let next = line_end + 1;
                    if next <= lo {
                        break;
                    }
                    lo = next;
                }
                std::cmp::Ordering::Greater => {
                    if line_start >= hi {
                        break;
                    }
                    hi = line_start;
                }
            }
        }
        None
    }

    /// Build an index by walking `root`.
    pub fn build(root: &Path) -> RepoIndex {
        let files = walk::source_files(root);
        let mut terms: HashMap<String, TermStat> = HashMap::new();
        let mut indexed = 0usize;

        for path in &files {
            // Non-UTF-8 files are skipped rather than lossily decoded: a
            // mis-decoded binary would inject thousands of junk terms.
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            extract::index_text(&text, &mut terms);
            indexed += 1;
        }

        // Paths are referents in their own right. A prompt saying
        // `src/auth/login.rs` names exactly one thing and should resolve
        // perfectly, so the full relative path and the bare file name are
        // both indexed as definitions -- a path denotes one file the way a
        // `fn` denotes one function. Two directories holding a `mod.rs` then
        // correctly make that bare name ambiguous, while the full path stays
        // unambiguous.
        for path in &files {
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let text = relative.to_string_lossy().replace('\\', "/").to_lowercase();

            for name in [Some(text.as_str()), text.rsplit('/').next()]
                .into_iter()
                .flatten()
                .collect::<std::collections::HashSet<_>>()
            {
                if name.is_empty() || name.len() > MAX_PATH_TERM {
                    continue;
                }
                let entry = terms.entry(name.to_string()).or_default();
                entry.df = entry.df.saturating_add(1);
                entry.cf = entry.cf.saturating_add(1);
                entry.def = entry.def.saturating_add(1);
            }

            // The individual words of the path as well, so "login" registers
            // as appearing here even when written as prose.
            let mut per_path = HashMap::new();
            extract::index_text(&text, &mut per_path);
            for (term, stat) in per_path {
                let entry = terms.entry(term).or_default();
                entry.cf = entry.cf.saturating_add(stat.cf);
                entry.df = entry.df.saturating_add(stat.df);
            }
        }

        Self::from_terms(indexed, terms)
    }

    fn from_terms(files: usize, terms: HashMap<String, TermStat>) -> RepoIndex {
        let mut records: Vec<(String, TermStat)> = terms.into_iter().collect();
        records.sort_by(|a, b| a.0.cmp(&b.0));

        let total_terms: u64 = records.iter().map(|(_, s)| s.cf as u64).sum();
        let distinct_terms = records.len();

        let mut blob =
            format!("{MAGIC}\t{FORMAT_VERSION}\t{files}\t{total_terms}\t{distinct_terms}\n");
        let body = blob.len();
        for (term, stat) in &records {
            // A term carrying a tab or newline would corrupt the record format
            // and break the sort order that binary search depends on. Nothing
            // produces such a term today; this makes sure nothing ever can.
            if term.contains(['\t', '\n']) {
                debug_assert!(false, "unrepresentable term {term:?}");
                continue;
            }
            blob.push_str(&format!(
                "{}\t{}\t{}\t{}\n",
                term, stat.df, stat.cf, stat.def
            ));
        }

        RepoIndex {
            blob,
            body,
            files,
            total_terms,
            distinct_terms,
        }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Temp-and-rename so a hook reading concurrently never sees a
        // half-written index.
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        std::fs::write(&tmp, self.blob.as_bytes())?;
        match std::fs::rename(&tmp, path) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    /// Read an index from disk. Returns `None` for anything unreadable or
    /// written by a different format version -- callers degrade rather than
    /// fail.
    pub fn load(path: &Path) -> Option<RepoIndex> {
        let blob = std::fs::read_to_string(path).ok()?;
        Self::parse(blob)
    }

    fn parse(blob: String) -> Option<RepoIndex> {
        let header_end = blob.find('\n')?;
        let header = &blob[..header_end];
        let mut fields = header.split('\t');
        if fields.next()? != MAGIC {
            return None;
        }
        if fields.next()?.parse::<u32>().ok()? != FORMAT_VERSION {
            return None;
        }
        let files = fields.next()?.parse().ok()?;
        let total_terms = fields.next()?.parse().ok()?;
        let distinct_terms = fields.next()?.parse().ok()?;

        Some(RepoIndex {
            body: header_end + 1,
            blob,
            files,
            total_terms,
            distinct_terms,
        })
    }
}

fn parse_record(line: &str) -> Option<TermStat> {
    let mut fields = line.split('\t');
    let _term = fields.next()?;
    Some(TermStat {
        df: fields.next()?.parse().ok()?,
        cf: fields.next()?.parse().ok()?,
        def: fields.next()?.parse().ok()?,
    })
}

/// A stable file name for the index of `root`.
///
/// FNV-1a over the path: short, dependency-free, and collisions only ever
/// mean a stale index for one directory, never a wrong answer for another --
/// the index is a cache, not a source of truth.
pub fn index_file_name(root: &Path) -> String {
    let text = root.to_string_lossy();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{hash:016x}.tsv")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_of(pairs: &[(&str, u32, u32, u32)]) -> RepoIndex {
        let terms: HashMap<String, TermStat> = pairs
            .iter()
            .map(|(t, df, cf, def)| {
                (
                    t.to_string(),
                    TermStat {
                        df: *df,
                        cf: *cf,
                        def: *def,
                    },
                )
            })
            .collect();
        RepoIndex::from_terms(3, terms)
    }

    #[test]
    fn finds_every_term_it_stored() {
        let pairs: Vec<(String, u32, u32, u32)> = (0..500)
            .map(|i| (format!("term{i:04}"), i as u32, i as u32 * 2, i as u32 % 3))
            .collect();
        let borrowed: Vec<(&str, u32, u32, u32)> = pairs
            .iter()
            .map(|(t, a, b, c)| (t.as_str(), *a, *b, *c))
            .collect();
        let index = index_of(&borrowed);

        for (term, df, cf, def) in &borrowed {
            let found = index
                .lookup(term)
                .unwrap_or_else(|| panic!("{term} not found"));
            assert_eq!(
                found,
                TermStat {
                    df: *df,
                    cf: *cf,
                    def: *def
                },
                "{term}"
            );
        }
    }

    #[test]
    fn does_not_invent_terms_it_never_stored() {
        let index = index_of(&[("alpha", 1, 1, 0), ("gamma", 2, 2, 1), ("omega", 3, 3, 0)]);
        for missing in [
            "",
            "a",
            "beta",
            "delta",
            "zzz",
            "alphaa",
            "alph",
            "\u{1F600}",
        ] {
            assert!(index.lookup(missing).is_none(), "invented {missing:?}");
        }
    }

    #[test]
    fn lookup_is_case_insensitive() {
        let index = index_of(&[("verify_token", 1, 4, 1)]);
        assert!(index.lookup("VERIFY_TOKEN").is_some());
        assert!(index.lookup("Verify_Token").is_some());
    }

    #[test]
    fn an_empty_index_answers_nothing_rather_than_hanging() {
        let index = index_of(&[]);
        assert!(index.is_empty());
        assert!(index.lookup("anything").is_none());
    }

    #[test]
    fn a_single_entry_index_works() {
        let index = index_of(&[("only", 1, 1, 1)]);
        assert!(index.lookup("only").is_some());
        assert!(index.lookup("other").is_none());
    }

    #[test]
    fn survives_a_save_and_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("yp-index-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("test.tsv");

        let index = index_of(&[("alpha", 1, 2, 0), ("beta", 3, 4, 1)]);
        index.save(&path).unwrap();

        let loaded = RepoIndex::load(&path).expect("should load");
        assert_eq!(loaded.files(), 3);
        assert_eq!(loaded.distinct_terms(), 2);
        assert_eq!(loaded.total_terms(), 6);
        assert_eq!(loaded.lookup("beta").unwrap().def, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_a_corrupt_or_foreign_index() {
        assert!(RepoIndex::parse("garbage".to_string()).is_none());
        assert!(RepoIndex::parse("#OTHER\t1\t1\t1\t1\n".to_string()).is_none());
        assert!(RepoIndex::parse("#YPIX\t999\t1\t1\t1\n".to_string()).is_none());
        assert!(RepoIndex::parse(String::new()).is_none());
    }

    #[test]
    fn index_file_names_are_stable_and_distinct() {
        let a = index_file_name(Path::new("/home/me/project-a"));
        let b = index_file_name(Path::new("/home/me/project-b"));
        assert_eq!(a, index_file_name(Path::new("/home/me/project-a")));
        assert_ne!(a, b);
        assert!(a.ends_with(".tsv"));
    }

    #[test]
    fn a_source_path_resolves_to_exactly_one_thing() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let index = RepoIndex::build(root);

        // The full relative path names one file and must resolve cleanly.
        let full = index
            .lookup("crates/yp-index/src/lib.rs")
            .expect("full path should be indexed");
        assert_eq!(full.def, 1, "a path denotes exactly one file: {full:?}");

        // A bare file name several directories share must not.
        let shared = index.lookup("lib.rs").expect("bare name should be indexed");
        assert!(
            shared.def > 1,
            "several crates have a lib.rs, so it is ambiguous: {shared:?}"
        );
    }

    #[test]
    fn indexing_this_repository_produces_a_usable_index() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let index = RepoIndex::build(root);

        assert!(index.files() > 5, "indexed {} files", index.files());
        assert!(index.distinct_terms() > 100);

        // Names this very file defines must be findable, and marked as
        // definitions rather than mere mentions.
        let stat = index.lookup("index_file_name").expect("own function");
        assert!(stat.def >= 1, "expected a definition site, got {stat:?}");

        // A subword shared by many identifiers must look ambiguous.
        let index_word = index.lookup("index").expect("subword");
        assert!(
            index_word.df > stat.df,
            "the generic word should appear in more files: {index_word:?} vs {stat:?}"
        );
    }
}
