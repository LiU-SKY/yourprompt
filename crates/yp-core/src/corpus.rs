//! What the grounding axis needs to know about a repository.
//!
//! A trait rather than a direct dependency on `yp-index`, for two reasons:
//! `yp-core` stays a pure function of its inputs with no file access, and the
//! axis can be tested against hand-built corpora where every frequency is
//! known exactly.

/// How a term appears across a repository.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TermFacts {
    /// Number of files containing the term.
    pub df: u32,
    /// Total occurrences across all files.
    pub cf: u32,
    /// Occurrences in a definition position (`fn foo`, `class Foo`, ...).
    pub def: u32,
}

impl TermFacts {
    /// How many distinct things this term could plausibly denote.
    ///
    /// A name that is *defined* once and used in forty files is not
    /// ambiguous -- there is exactly one thing to change. A name that is
    /// defined nowhere and merely appears in forty files is: the agent has no
    /// way to know which of them the user meant. So definition sites win when
    /// there are any, and document frequency stands in when there are none.
    pub fn candidates(&self) -> u32 {
        if self.def > 0 {
            self.def
        } else {
            self.df
        }
    }
}

/// A repository, as the grounding axis sees it.
pub trait Corpus {
    fn lookup(&self, term: &str) -> Option<TermFacts>;
    /// How many files the index covers.
    fn documents(&self) -> usize;
    /// Total term occurrences, the denominator of the collection model.
    fn total_terms(&self) -> u64;
    /// True when the files were indexed under bare names, with no directory
    /// in any of them.
    ///
    /// A browser hands the demo page a picked or dropped file as `login.rs`,
    /// never as `src/auth/login.rs`, so a prompt naming the file by its path
    /// was told the file was not in the code it had just been given: the
    /// showcase prompt scored 772.2 against the sample's six files under bare
    /// names and 851.8 against the same files under their paths. A path in
    /// the prompt is then looked up by its last segment instead.
    ///
    /// An index built by walking a repository has the real paths, so a path
    /// it does not contain is genuinely missing. Only an index that says so
    /// is flat.
    fn flat(&self) -> bool {
        false
    }

    /// True when a file called `name` was handed over without a directory.
    ///
    /// The same question as [`flat`](Self::flat), asked of one file. The page
    /// mixes the two kinds: a file picked on its own arrives as `login.rs`, a
    /// folder added beside it keeps `tests/auth.rs`. Decided for the whole
    /// index, one directory anywhere turned the fallback off, and the
    /// showcase prompt with `login.rs` and `tests/auth.rs` loaded scored
    /// 837.4, with `src/auth/login.rs` "not in this repository", against 865.6
    /// with both under bare names. Only a path whose file name arrived bare
    /// is looked up by that name: `src/auth.rs` is still missing where the
    /// only `auth.rs` is `tests/auth.rs`.
    fn bare_file(&self, _name: &str) -> bool {
        self.flat()
    }

    /// What the indexed paths that end in `tail` after a `/` denote, counted
    /// together, or `None` when none does.
    ///
    /// A prompt gives as much of a path as its writer thought enough:
    /// `auth/login.rs` for `src/auth/login.rs` was "not in this repository",
    /// and against the demo's sample "Fix the expired-token panic in
    /// auth/login.rs." scored 378.5 where the whole path scores 644.3. The
    /// tail names the file when one path ends in it, and could be any of them
    /// when several do, as a bare file name could. A path at the root has no
    /// `/` in front of its name and is not counted.
    ///
    /// A corpus that cannot list its paths answers `None`, and a partial path
    /// is then missing, as it always was.
    fn paths_ending_in(&self, _tail: &str) -> Option<TermFacts> {
        None
    }
}

/// A corpus built from a literal list, for tests and examples.
#[derive(Debug, Default, Clone)]
pub struct MapCorpus {
    terms: std::collections::HashMap<String, TermFacts>,
    documents: usize,
    total_terms: u64,
    flat: bool,
    bare: std::collections::HashSet<String>,
}

impl MapCorpus {
    /// Build from `(term, df, cf, def)` tuples.
    pub fn new(documents: usize, entries: &[(&str, u32, u32, u32)]) -> Self {
        let terms: std::collections::HashMap<String, TermFacts> = entries
            .iter()
            .map(|(t, df, cf, def)| {
                (
                    t.to_lowercase(),
                    TermFacts {
                        df: *df,
                        cf: *cf,
                        def: *def,
                    },
                )
            })
            .collect();
        let total_terms = terms.values().map(|f| f.cf as u64).sum::<u64>().max(1);
        MapCorpus {
            terms,
            documents,
            total_terms,
            flat: false,
            bare: std::collections::HashSet::new(),
        }
    }

    /// The same corpus, as if its files had been indexed under bare names.
    pub fn with_bare_names(mut self) -> Self {
        self.flat = true;
        self
    }

    /// The same corpus, as if the file called `name` alone had been handed
    /// over without its directory.
    pub fn with_bare_file(mut self, name: &str) -> Self {
        self.bare.insert(name.to_lowercase());
        self
    }
}

impl Corpus for MapCorpus {
    fn lookup(&self, term: &str) -> Option<TermFacts> {
        self.terms.get(&term.to_lowercase()).copied()
    }

    fn documents(&self) -> usize {
        self.documents
    }

    fn total_terms(&self) -> u64 {
        self.total_terms
    }

    fn flat(&self) -> bool {
        self.flat
    }

    fn bare_file(&self, name: &str) -> bool {
        self.flat || self.bare.contains(&name.to_lowercase())
    }

    fn paths_ending_in(&self, tail: &str) -> Option<TermFacts> {
        let end = format!("/{}", tail.to_lowercase());
        self.terms
            .iter()
            .filter(|(term, _)| term.ends_with(&end))
            .map(|(_, facts)| *facts)
            .reduce(|a, b| TermFacts {
                df: a.df.saturating_add(b.df),
                cf: a.cf.saturating_add(b.cf),
                def: a.def.saturating_add(b.def),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_defined_once_is_unambiguous_however_widely_it_is_used() {
        let facts = TermFacts {
            df: 40,
            cf: 200,
            def: 1,
        };
        assert_eq!(facts.candidates(), 1);
    }

    #[test]
    fn a_name_defined_nowhere_is_as_ambiguous_as_its_spread() {
        let facts = TermFacts {
            df: 37,
            cf: 90,
            def: 0,
        };
        assert_eq!(facts.candidates(), 37);
    }

    #[test]
    fn map_corpus_lookup_is_case_insensitive() {
        let corpus = MapCorpus::new(10, &[("verify_token", 3, 9, 1)]);
        assert_eq!(corpus.lookup("VERIFY_TOKEN").unwrap().def, 1);
        assert!(corpus.lookup("absent").is_none());
        assert_eq!(corpus.documents(), 10);
        assert_eq!(corpus.total_terms(), 9);
    }

    #[test]
    fn a_corpus_has_paths_unless_it_says_otherwise() {
        let corpus = MapCorpus::new(1, &[("login.rs", 1, 1, 1)]);
        assert!(!corpus.flat());
        assert!(!corpus.bare_file("login.rs"));
        let flat = corpus.clone().with_bare_names();
        assert!(flat.flat());
        assert!(flat.bare_file("login.rs"));

        // One file handed over bare says nothing about the others.
        let mixed = corpus.with_bare_file("Login.rs");
        assert!(!mixed.flat());
        assert!(mixed.bare_file("login.rs"));
        assert!(!mixed.bare_file("auth.rs"));
    }

    #[test]
    fn a_path_is_found_by_its_end_after_a_slash() {
        let corpus = MapCorpus::new(
            3,
            &[
                ("Cargo.toml", 1, 1, 1),
                ("crates/api/Cargo.toml", 1, 1, 1),
                ("src/auth/login.rs", 1, 1, 1),
                ("src/oauth/login.rs", 1, 1, 1),
                ("src/auth", 1, 1, 1),
            ],
        );
        let ending = |tail: &str| corpus.paths_ending_in(tail).map(|f| f.def);
        assert_eq!(ending("auth/login.rs"), Some(1));
        assert_eq!(ending("AUTH/Login.rs"), Some(1));
        assert_eq!(ending("login.rs"), Some(2));
        assert_eq!(ending("src/auth"), None, "a whole path is not its own end");
        assert_eq!(ending("auth"), Some(1));
        // The root `Cargo.toml` has no `/` before its name.
        assert_eq!(ending("Cargo.toml"), Some(1));
        assert_eq!(ending("th/login.rs"), None);
        assert_eq!(ending("logout.rs"), None);
        // A corpus that cannot list its paths finds none.
        struct Opaque;
        impl Corpus for Opaque {
            fn lookup(&self, _: &str) -> Option<TermFacts> {
                None
            }
            fn documents(&self) -> usize {
                0
            }
            fn total_terms(&self) -> u64 {
                0
            }
        }
        assert_eq!(Opaque.paths_ending_in("auth/login.rs"), None);
    }
}
