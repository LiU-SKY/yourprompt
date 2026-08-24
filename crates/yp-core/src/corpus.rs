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
}

/// A corpus built from a literal list, for tests and examples.
#[derive(Debug, Default, Clone)]
pub struct MapCorpus {
    terms: std::collections::HashMap<String, TermFacts>,
    documents: usize,
    total_terms: u64,
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
        }
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
}
