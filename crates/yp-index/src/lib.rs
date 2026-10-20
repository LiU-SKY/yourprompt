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

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;

pub use extract::TermStat;

const MAGIC: &str = "#YPIX";
const FORMAT_VERSION: u32 = 2;

/// Longest path kept as a single term. Identifiers are capped much shorter,
/// but a real source path easily exceeds that and is exactly the kind of
/// referent worth resolving.
const MAX_PATH_TERM: usize = 256;

/// Written after the header's counts when every file was indexed under a bare
/// name. An index without it, including every index written before it
/// existed, has paths.
const FLAT_MARK: &str = "flat";

/// Written as a record's fifth field when a file was handed over under that
/// bare name, so the page can mix a picked `login.rs` with a dropped
/// `tests/auth.rs` and still know which of the two has a directory nobody
/// told it. A reader that predates the field reads the first four and stops,
/// so the format version is unchanged.
const BARE_MARK: &str = "bare";

/// A queryable repository index.
pub struct RepoIndex {
    /// The whole file, header included.
    blob: String,
    /// Byte offset of the first record, just past the header line.
    body: usize,
    files: usize,
    total_terms: u64,
    distinct_terms: usize,
    flat: bool,
}

impl RepoIndex {
    pub fn files(&self) -> usize {
        self.files
    }

    /// True when the files were handed over under bare names, so no indexed
    /// path has a directory in it. A walked repository never is, even when
    /// every file sits at its root: those are the real paths.
    pub fn is_flat(&self) -> bool {
        self.flat
    }

    /// True when a file called `name` was handed over without a directory,
    /// whatever the other files came with. Case-insensitive.
    pub fn is_bare(&self, name: &str) -> bool {
        self.flat
            || self
                .record(name)
                .is_some_and(|line| line.split('\t').nth(4) == Some(BARE_MARK))
    }

    /// Total term occurrences across the repository -- the denominator of the
    /// collection language model the specificity score is built on.
    pub fn total_terms(&self) -> u64 {
        self.total_terms
    }

    pub fn distinct_terms(&self) -> usize {
        self.distinct_terms
    }

    /// The index in its on-disk form, for handing to a caller that has no
    /// filesystem to save it to.
    pub fn as_str(&self) -> &str {
        &self.blob
    }

    pub fn is_empty(&self) -> bool {
        self.distinct_terms == 0
    }

    /// Look one term up. Case-insensitive: the index stores lowercase.
    pub fn lookup(&self, term: &str) -> Option<TermStat> {
        self.record(term).and_then(parse_record)
    }

    /// The whole record line for `term`, if the index has one.
    fn record(&self, term: &str) -> Option<&str> {
        let needle = term.to_lowercase();
        let body = &self.blob[self.body..];
        // The search halves a byte range, so `mid` can fall inside a
        // multi-byte character of a non-ASCII path, and slicing the str there
        // panicked. Newlines are looked for in the bytes instead: '\n' never
        // occurs inside a UTF-8 sequence, so every line found still starts
        // and ends on a character boundary.
        let bytes = body.as_bytes();
        if bytes.is_empty() {
            return None;
        }

        let mut lo = 0usize;
        let mut hi = bytes.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            // Snap to the start of whichever line `mid` landed inside.
            let line_start = bytes[..mid]
                .iter()
                .rposition(|&b| b == b'\n')
                .map_or(0, |i| i + 1);
            let line_end = bytes[line_start..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(bytes.len(), |i| line_start + i);
            let line = &body[line_start..line_end];
            let key = line.split('\t').next().unwrap_or("");

            match key.cmp(needle.as_str()) {
                std::cmp::Ordering::Equal => return Some(line),
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
        let paths = walk::source_files(root);
        let files = walk::read_sources(root, &paths);
        let files: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();

        // A walk yields real paths. A file at the root of the repository is
        // named by its bare name because that is its path.
        Self::index_files(&files, false, &HashSet::new())
    }

    /// Build an index from files already in memory, as `(path, contents)`.
    ///
    /// The same index the filesystem walk produces, minus the walking. That
    /// is what makes the browser build possible, where there is no filesystem
    /// and the repository is whatever the visitor dropped on the page.
    ///
    /// A browser names a file picked on its own without its directory, so
    /// each name that has none is marked [bare](Self::is_bare) whatever the
    /// others came with, and when no name has one the index is marked
    /// [flat](Self::is_flat).
    ///
    /// A file at the root of a folder has a directory, the root, and is
    /// written `./Cargo.toml` to say so. It is indexed as `Cargo.toml` with
    /// neither mark, as the walk indexes it. Under its bare name, any
    /// `<dir>/Cargo.toml` a prompt made up was taken for it: the page said
    /// "crates/api/Cargo.toml" was defined once in the demo's sample, where
    /// `yp index` of the same six files has it missing, 644.3 against 378.5.
    /// A name without the prefix is read as before, so an older page's files
    /// are indexed as they always were.
    pub fn from_files(files: &[(String, String)]) -> RepoIndex {
        let mut bare = HashSet::new();
        let mut flat = !files.is_empty();
        let files: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, text)| {
                let rooted = path.strip_prefix("./").or_else(|| path.strip_prefix(".\\"));
                if rooted.is_some() || path.contains(['/', '\\']) {
                    flat = false;
                } else {
                    bare.insert(path.to_lowercase());
                }
                (rooted.unwrap_or(path), text.as_str())
            })
            .collect();
        Self::index_files(&files, flat, &bare)
    }

    fn index_files(files: &[(&str, &str)], flat: bool, bare: &HashSet<String>) -> RepoIndex {
        let mut terms: HashMap<String, TermStat> = HashMap::new();
        let indexed = files.len();

        for (_, text) in files {
            extract::index_text(text, &mut terms);
        }

        // Paths are referents in their own right. A prompt saying
        // `src/auth/login.rs` names exactly one thing and should resolve
        // perfectly, so the full relative path and the bare file name are
        // both indexed as definitions -- a path denotes one file the way a
        // `fn` denotes one function. Two directories holding a `mod.rs` then
        // correctly make that bare name ambiguous, while the full path stays
        // unambiguous.
        for (path, _) in files {
            let text = path.to_lowercase();

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

        // Directories, too: "add a test in crates/yp-cli" names one place,
        // and used to be reported as "not in this repository". Only nested
        // directories are added, so a top-level "src" or "docs" stays a prose
        // word and is not promoted to a definition.
        let mut dirs: HashMap<String, u32> = HashMap::new();
        for (path, _) in files {
            let text = path.to_lowercase();
            let mut end = 0;
            while let Some(i) = text[end..].find('/') {
                end += i;
                let dir = &text[..end];
                if dir.contains('/') && dir.len() <= MAX_PATH_TERM {
                    *dirs.entry(dir.to_string()).or_default() += 1;
                }
                end += 1;
            }
        }
        for (dir, count) in dirs {
            let entry = terms.entry(dir).or_default();
            entry.df = entry.df.saturating_add(count);
            entry.cf = entry.cf.saturating_add(count);
            entry.def = entry.def.saturating_add(1);
        }

        Self::from_terms(indexed, terms, flat, bare)
    }

    fn from_terms(
        files: usize,
        terms: HashMap<String, TermStat>,
        flat: bool,
        bare: &HashSet<String>,
    ) -> RepoIndex {
        let mut records: Vec<(String, TermStat)> = terms.into_iter().collect();
        records.sort_by(|a, b| a.0.cmp(&b.0));

        let total_terms: u64 = records.iter().map(|(_, s)| s.cf as u64).sum();
        let distinct_terms = records.len();

        // The page builds an index, hands it to JavaScript as text and gets
        // it back with every score, so whether it is flat has to travel in
        // the text. An extra header field is ignored by a reader that does not
        // know it, and an index with paths is written exactly as before.
        let mut blob =
            format!("{MAGIC}\t{FORMAT_VERSION}\t{files}\t{total_terms}\t{distinct_terms}");
        if flat {
            blob.push('\t');
            blob.push_str(FLAT_MARK);
        }
        blob.push('\n');
        let body = blob.len();
        for (term, stat) in &records {
            // A term carrying a tab or newline would corrupt the record format
            // and break the sort order that binary search depends on. Nothing
            // produces such a term today; this makes sure nothing ever can.
            if term.contains(['\t', '\n']) {
                debug_assert!(false, "unrepresentable term {term:?}");
                continue;
            }
            blob.push_str(&format!("{}\t{}\t{}\t{}", term, stat.df, stat.cf, stat.def));
            if bare.contains(term) {
                blob.push('\t');
                blob.push_str(BARE_MARK);
            }
            blob.push('\n');
        }

        RepoIndex {
            blob,
            body,
            files,
            total_terms,
            distinct_terms,
            flat,
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

    /// Parse an index that is already in memory.
    ///
    /// Used where there is no filesystem to read from -- the browser build
    /// receives the index as bytes over the network.
    pub fn parse_str(blob: &str) -> Option<RepoIndex> {
        Self::parse(blob.to_string())
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
        let flat = fields.next() == Some(FLAT_MARK);

        Some(RepoIndex {
            body: header_end + 1,
            blob,
            files,
            total_terms,
            distinct_terms,
            flat,
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
        RepoIndex::from_terms(3, terms, false, &HashSet::new())
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
    fn non_ascii_paths_never_split_a_character() {
        // Identifiers are ASCII, but paths and file names are indexed whole,
        // so a step of the binary search can land inside a multi-byte
        // character. Slicing the index there panicked: a file named 보고서.md
        // was enough to crash the hook and the demo page.
        let files: Vec<(String, String)> = [
            ("docs/보고서.md", "# 메모\n"),
            ("실행해", ""),
            ("docs/설계/회의록 2024.md", "fn verify_token() {}"),
            ("設計/メモ.md", "struct AuthError;"),
            ("ドキュメント/仕様書.txt", "parse_args"),
            ("src/café/résumé.rs", "fn crème_brûlée() {}"),
            ("Ünïcödé/ÄÖÜ straße.py", "def naive_path(): pass"),
            ("assets/🎉 party/🎊.txt", "launch_party"),
            ("emoji/👨‍👩‍👧‍👦.md", ""),
            ("src/auth/login.rs", "pub fn login() {}"),
        ]
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect();
        let index = RepoIndex::from_files(&files);

        // Sorted, so the needles below are the same on every run.
        let stored: std::collections::BTreeMap<&str, TermStat> = index
            .as_str()
            .lines()
            .skip(1)
            .map(|line| {
                (
                    line.split('\t').next().unwrap(),
                    parse_record(line).unwrap(),
                )
            })
            .collect();
        assert!(stored.contains_key("docs/보고서.md"), "{stored:?}");
        for (term, stat) in &stored {
            assert_eq!(index.lookup(term), Some(*stat), "{term}");
        }

        // Needles built from the same characters, most of them not stored:
        // each probes a different sequence of midpoints.
        let alphabet: Vec<char> = "ae_/. 보고서실행設計メモé🎉ÄÖÜß".chars().collect();
        let terms: Vec<&str> = stored.keys().copied().collect();
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = move |below: usize| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % below as u64) as usize
        };
        for round in 0..600 {
            let mut needle: String = (0..next(12))
                .map(|_| alphabet[next(alphabet.len())])
                .collect();
            if round % 2 == 0 {
                // A stored term with something added: the nearest misses.
                needle.insert_str(0, terms[next(terms.len())]);
            }
            assert_eq!(
                index.lookup(&needle),
                stored.get(needle.to_lowercase().as_str()).copied(),
                "{needle:?}"
            );
        }
    }

    #[test]
    fn a_nested_directory_is_a_definition() {
        let files = vec![
            ("crates/yp-cli/src/main.rs".to_string(), String::new()),
            ("crates/yp-cli/src/hook.rs".to_string(), String::new()),
        ];
        let index = RepoIndex::from_files(&files);
        let dir = index.lookup("crates/yp-cli").expect("directory indexed");
        assert_eq!(dir.def, 1);
        assert_eq!(dir.df, 2);
        // A top-level directory stays a prose word.
        assert!(index.lookup("crates").is_none_or(|t| t.def == 0));
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
    fn an_index_can_be_built_from_files_in_memory() {
        // The browser has no filesystem to walk, so this path has to produce
        // the same index the walk does.
        let files = vec![
            (
                "src/auth/login.rs".to_string(),
                "pub fn verify_token(t: &str) -> bool { t.len() > 3 }".to_string(),
            ),
            (
                "src/main.rs".to_string(),
                "fn main() { verify_token(\"x\"); }".to_string(),
            ),
        ];
        let index = RepoIndex::from_files(&files);

        assert_eq!(index.files(), 2);
        // Defined once, used twice.
        let token = index.lookup("verify_token").expect("the function");
        assert_eq!(token.def, 1);
        assert_eq!(token.df, 2);
        // The path itself resolves to exactly one file.
        assert_eq!(index.lookup("src/auth/login.rs").expect("the path").def, 1);
        assert!(index.lookup("nothing_here").is_none());
    }

    #[test]
    fn files_handed_over_under_bare_names_make_a_flat_index() {
        let file = |name: &str| (name.to_string(), "fn verify_token() {}".to_string());

        let picked = RepoIndex::from_files(&[file("login.rs"), file("auth.rs")]);
        assert!(picked.is_flat());
        // The page keeps the index as text between scores.
        let reread = RepoIndex::parse_str(picked.as_str()).expect("parses");
        assert!(reread.is_flat());
        assert_eq!(reread.files(), 2);
        assert_eq!(reread.lookup("login.rs").expect("the file").def, 1);

        // One directory anywhere means the names are paths.
        for files in [
            vec![file("login.rs"), file("tests/auth.rs")],
            vec![file("src\\auth\\login.rs")],
        ] {
            let index = RepoIndex::from_files(&files);
            assert!(!index.is_flat());
            assert!(!RepoIndex::parse_str(index.as_str()).unwrap().is_flat());
        }
        assert!(!RepoIndex::from_files(&[]).is_flat());
    }

    #[test]
    fn a_file_handed_over_bare_is_known_by_name_beside_files_with_paths() {
        // What the page holds once a picked file and a dropped folder mix.
        let file = |name: &str| (name.to_string(), "fn verify_token() {}".to_string());
        let mixed = RepoIndex::from_files(&[file("login.rs"), file("tests/auth.rs")]);
        assert!(!mixed.is_flat());
        for index in [RepoIndex::parse_str(mixed.as_str()).expect("parses"), mixed] {
            assert!(index.is_bare("login.rs"));
            assert!(index.is_bare("Login.RS"));
            assert!(!index.is_bare("auth.rs"));
            assert!(!index.is_bare("tests/auth.rs"));
            assert!(!index.is_bare("verify_token"));
            assert!(!index.is_bare("absent.rs"));
            // The mark changes no count.
            let one = TermStat {
                df: 1,
                cf: 1,
                def: 1,
            };
            assert_eq!(index.lookup("login.rs"), Some(one));
            assert_eq!(index.lookup("auth.rs"), Some(one));
        }

        // A bare file and a pathed one can share a name, and then the name
        // was handed over bare.
        let shared = RepoIndex::from_files(&[file("auth.rs"), file("tests/auth.rs")]);
        assert!(shared.is_bare("auth.rs"));
        assert_eq!(shared.lookup("auth.rs").unwrap().def, 2);

        // Every name in a flat index is bare, including in one written
        // before the field existed.
        let old = RepoIndex::parse_str("#YPIX\t2\t1\t1\t1\tflat\nlogin.rs\t1\t1\t1\n").unwrap();
        assert!(old.is_bare("login.rs"));
        let old = RepoIndex::parse_str("#YPIX\t2\t1\t1\t1\nlogin.rs\t1\t1\t1\n").unwrap();
        assert!(!old.is_bare("login.rs"));
        assert_eq!(old.lookup("login.rs").unwrap().def, 1);
    }

    #[test]
    fn a_file_at_the_root_of_a_folder_is_indexed_as_the_walk_indexes_it() {
        // The page sends the sample's Cargo.toml, and the root files of a
        // dropped folder, as ./Cargo.toml.
        let files = [
            ("Cargo.toml", "[package]\nname = \"acme-auth\"\n"),
            ("src/lib.rs", "pub fn verify_token() {}\n"),
            ("tests/auth.rs", "fn expired() { verify_token(); }\n"),
        ];
        let root = std::env::temp_dir().join(format!("yp-index-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (path, text) in files {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let walked = RepoIndex::build(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(walked.files(), 3);

        let sent = |prefix: &str| -> Vec<(String, String)> {
            files
                .iter()
                .map(|(path, text)| {
                    let name = if path.contains('/') {
                        path.to_string()
                    } else {
                        format!("{prefix}{path}")
                    };
                    (name, text.to_string())
                })
                .collect()
        };
        for prefix in ["./", ".\\"] {
            let page = RepoIndex::from_files(&sent(prefix));
            assert_eq!(page.as_str(), walked.as_str(), "{prefix}");
            assert!(!page.is_bare("Cargo.toml"));
            assert!(page.lookup(&format!("{prefix}cargo.toml")).is_none());
        }

        // Sent bare, as a page did before the prefix, the same file is taken
        // for one picked on its own, and that is how it is still read.
        let old = RepoIndex::from_files(&sent(""));
        assert!(old.is_bare("Cargo.toml"));
        assert_ne!(old.as_str(), walked.as_str());

        // A folder holding nothing but root files has paths all the same.
        let file = |name: &str| (name.to_string(), "def run(): pass\n".to_string());
        let shallow = RepoIndex::from_files(&[file("./main.py"), file("./setup.py")]);
        assert!(!shallow.is_flat());
        assert!(!shallow.is_bare("main.py"));
        assert_eq!(shallow.lookup("main.py").unwrap().def, 1);
        // Two files picked under one name are still a flat pair.
        assert!(RepoIndex::from_files(&[file("main.py"), file("main.py")]).is_flat());
    }

    #[test]
    fn a_walked_repository_is_never_flat() {
        // Every file here sits at the root, so no indexed name has a
        // directory, and those names are still the real paths.
        let root = std::env::temp_dir().join(format!("yp-index-flat-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("main.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("lib.rs"), "pub fn verify_token() {}").unwrap();

        let index = RepoIndex::build(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(index.files(), 2);
        assert!(!index.is_flat());
        assert!(!index.is_bare("main.rs"));
        // Written exactly as an index was before either mark existed.
        assert_eq!(
            index.as_str().lines().next().unwrap().split('\t').count(),
            5
        );
        assert!(index
            .as_str()
            .lines()
            .skip(1)
            .all(|line| line.split('\t').count() == 4));
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
