//! Axis A -- does this prompt name things that exist, exactly once, *here*?
//!
//! This is the axis no other prompt scorer has. Every one of them grades a
//! prompt in a vacuum, where "fix the login handler" is simply a bit vague.
//! Conditioned on a repository it stops being a matter of taste: if `login`
//! matches thirty-seven places and none of them is a definition, the agent
//! genuinely cannot tell which one you meant, and the number says so.
//!
//! Three sub-scores:
//!
//! - **resolution** -- for each thing the prompt names, how many candidates
//!   does the repository offer? One is perfect, none is unresolvable, forty
//!   is a coin flip with thirty-nine losing sides.
//! - **specificity** -- the Simplified Clarity Score from the pre-retrieval
//!   query-performance-prediction literature (Hauff et al., CIKM 2008),
//!   computed against the repository's own vocabulary rather than a generic
//!   corpus. Words that are rare *here* carry more information *here*.
//! - **deixis** -- "it", "that", "그거" with nothing in the prompt to attach
//!   to.

use yp_lang::{SmellId, Span, Token, TokenKind};

use crate::axes::{Axis, Component};
use crate::corpus::Corpus;
use crate::params::{axis_max, decay, grounding as g, saturate};

/// One thing the prompt appears to name.
#[derive(Debug, Clone)]
pub struct Referent {
    pub text: String,
    /// Where it sits in the prompt, so deixis can ask what came before it.
    pub offset: usize,
    /// Explicit names weigh more than prose words that happen to exist.
    pub weight: f64,
    /// How many things in the repository it could denote. `None` when the
    /// repository has never heard of it.
    pub candidates: Option<u32>,
}

impl Referent {
    /// 1.0 for a name that lands on exactly one thing, falling off as the
    /// number of candidates grows, and 0.0 for a name the repository does not
    /// contain at all.
    ///
    /// The decay is logarithmic rather than linear: the difference between
    /// one candidate and two matters enormously, between thirty and forty
    /// hardly at all -- both are already hopeless.
    pub fn resolution(&self) -> f64 {
        match self.candidates {
            None | Some(0) => 0.0,
            Some(1) => 1.0,
            Some(n) => 1.0 / (1.0 + (n as f64).log2()),
        }
    }
}

/// Pick out the tokens that look like they name something.
///
/// Explicit names -- code spans, paths, identifiers -- always count. A plain
/// prose word counts only if the repository contains it *and* it is not so
/// widespread as to be vocabulary rather than a name.
pub fn referents(
    tokens: &[Token],
    instruction_words: &[Span],
    corpus: &dyn Corpus,
) -> Vec<Referent> {
    let documents = corpus.documents().max(1) as f64;
    let mut out = Vec::new();

    for token in tokens {
        let explicit = token.is_referent_candidate();
        if !explicit && token.kind != TokenKind::Word {
            continue;
        }

        if !explicit && instruction_words.iter().any(|s| s.overlaps(&token.span)) {
            // A word already doing duty as an instruction -- "fix", "return",
            // "only" -- is not the user naming something. Repositories are
            // full of the word "fix"; counting it as a referent would let the
            // verb of the sentence drag down the score for the nouns.
            continue;
        }

        let facts = corpus.lookup(&token.text);

        if !explicit {
            // Prose. Only interesting if the repository knows the word and
            // does not use it everywhere.
            let Some(facts) = facts else { continue };
            if facts.df as f64 / documents > g::UBIQUITY_CUTOFF {
                continue;
            }
        }

        out.push(Referent {
            text: token.text.clone(),
            offset: token.span.start,
            weight: if explicit {
                g::EXPLICIT_WEIGHT
            } else {
                g::PROSE_WEIGHT
            },
            candidates: facts.map(|f| f.candidates()),
        });
    }
    out
}

/// Simplified Clarity Score: the divergence between the prompt's term
/// distribution and the repository's.
///
/// `SCS = sum over query terms of P(w|q) * log2( P(w|q) / P(w|C) )`
///
/// A prompt made of words that are common in this repository diverges little
/// and scores low; one that names something rare diverges sharply and scores
/// high. Unseen terms are smoothed rather than treated as impossible, so a
/// typo cannot send the score to infinity.
pub fn simplified_clarity_score(terms: &[String], corpus: &dyn Corpus) -> f64 {
    if terms.is_empty() {
        return 0.0;
    }
    let total = corpus.total_terms().max(1) as f64;
    let query_len = terms.len() as f64;

    let mut counts: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
    for term in terms {
        *counts.entry(term.as_str()).or_default() += 1.0;
    }

    let mut scs = 0.0;
    for (term, count) in counts {
        let p_query = count / query_len;
        let cf = corpus
            .lookup(term)
            .map(|f| f.cf as f64)
            .filter(|cf| *cf > 0.0)
            .unwrap_or(g::UNSEEN_TERM_WEIGHT);
        let p_collection = cf / total;
        scs += p_query * (p_query / p_collection).log2();
    }
    scs.max(0.0)
}

/// Deictics with nothing to point at.
///
/// A prompt that says "fix it" right after naming `verify_token` is perfectly
/// clear; one that opens with "fix it" and never names anything is not. So a
/// deictic counts as dangling when no referent appears before it -- and when
/// the prompt names nothing at all, every one of them dangles.
fn dangling_deixis(pronouns: &[usize], referents: &[Referent]) -> usize {
    let first_referent = referents
        .iter()
        .filter(|r| r.candidates.is_some_and(|c| c > 0))
        .map(|r| r.offset)
        .min();

    match first_referent {
        None => pronouns.len(),
        Some(first) => pronouns.iter().filter(|&&at| at < first).count(),
    }
}

/// Score axis A.
///
/// `pronoun_offsets` are the positions of vague-pronoun smells found by the
/// clarity pass, reused here rather than detected twice.
pub fn grounding(
    tokens: &[Token],
    pronoun_offsets: &[usize],
    instruction_words: &[Span],
    corpus: &dyn Corpus,
) -> (Axis, Vec<Referent>) {
    let found = referents(tokens, instruction_words, corpus);

    // ---- resolution ---------------------------------------------------
    let resolution = if found.is_empty() {
        Component::new(
            "resolution",
            0.0,
            g::RESOLUTION_MAX,
            "names nothing that exists in this repository",
        )
    } else {
        let weight: f64 = found.iter().map(|r| r.weight).sum();
        let earned: f64 = found.iter().map(|r| r.weight * r.resolution()).sum();
        let ratio = if weight > 0.0 { earned / weight } else { 0.0 };

        // Report the worst offender: the name the agent would struggle most
        // to pin down.
        let worst = found
            .iter()
            .filter(|r| r.resolution() < 1.0)
            .min_by(|a, b| a.resolution().total_cmp(&b.resolution()));
        // "Resolve" means the same thing in both branches: lands on exactly
        // one thing. Anything looser would flatter the score.
        let clean = found.iter().filter(|r| r.resolution() >= 1.0).count();
        let detail = match worst {
            Some(r) => match r.candidates {
                None | Some(0) => format!(
                    "{clean} of {} names resolve; \"{}\" is not in this repository",
                    found.len(),
                    r.text
                ),
                Some(n) => format!(
                    "{clean} of {} names resolve; \"{}\" could be any of {n}",
                    found.len(),
                    r.text
                ),
            },
            None => format!("all {} name(s) resolve to exactly one thing", found.len()),
        };
        Component::new(
            "resolution",
            g::RESOLUTION_MAX * ratio,
            g::RESOLUTION_MAX,
            detail,
        )
    };

    // ---- specificity ---------------------------------------------------
    let terms: Vec<String> = tokens
        .iter()
        .filter(|t| matches!(t.kind, TokenKind::Word | TokenKind::Ident | TokenKind::Path))
        .map(|t| t.text.to_lowercase())
        .collect();
    let scs = simplified_clarity_score(&terms, corpus);
    let specificity = Component::new(
        "specificity",
        g::SPECIFICITY_MAX * (1.0 - (-scs / g::SCS_HALF_LIFE).exp()),
        g::SPECIFICITY_MAX,
        format!(
            "clarity score {scs:.2} against {} files",
            corpus.documents()
        ),
    );

    // ---- deixis ---------------------------------------------------------
    let dangling = dangling_deixis(pronoun_offsets, &found);
    let deixis = Component::new(
        "deixis",
        decay(dangling, g::DEIXIS_MAX, g::DEIXIS_DECAY),
        g::DEIXIS_MAX,
        if dangling == 0 {
            "no unattached references".to_string()
        } else {
            format!("{dangling} reference(s) with nothing to point at")
        },
    );

    let axis = Axis::from_components(
        "grounding",
        axis_max::GROUNDING,
        vec![resolution, specificity, deixis],
    );
    (axis, found)
}

/// Positions of vague-pronoun hits, for [`grounding`].
pub fn pronoun_offsets(hits: &[yp_lang::SmellHit]) -> Vec<usize> {
    hits.iter()
        .filter(|h| h.id == SmellId::VaguePronoun)
        .map(|h| h.span.start)
        .collect()
}

// `saturate` is re-exported for symmetry with the other axes even though this
// one uses an explicit curve; silence the unused import in builds that do not
// reference it.
const _: fn(usize, f64, f64) -> f64 = saturate;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::MapCorpus;
    use yp_lang::tokenize;

    /// A repository where `verify_token` is defined once, `login` is smeared
    /// across many files, and `handler` is ordinary vocabulary.
    fn repo() -> MapCorpus {
        MapCorpus::new(
            100,
            &[
                ("verify_token", 3, 12, 1),
                ("verify", 20, 60, 0),
                ("token", 25, 140, 0),
                ("login", 37, 90, 0),
                ("handler", 44, 120, 0),
                ("the", 98, 5000, 0),
                ("src/auth/login.rs", 1, 1, 0),
                ("parse_args", 2, 8, 1),
            ],
        )
    }

    fn axis_of(text: &str) -> Axis {
        let corpus = repo();
        let tokens = tokenize(text);
        let resources = yp_lang::resources().unwrap();
        let offsets = pronoun_offsets(&resources.smells.find(text));
        grounding(&tokens, &offsets, &[], &corpus).0
    }

    fn component(axis: &Axis, id: &str) -> f64 {
        axis.components
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("no component {id}"))
            .earned
    }

    #[test]
    fn a_name_defined_once_resolves_perfectly() {
        let axis = axis_of("fix verify_token");
        assert_eq!(component(&axis, "resolution"), g::RESOLUTION_MAX);
    }

    #[test]
    fn a_name_smeared_across_the_repository_does_not() {
        // The headline case: "login" is in 37 files and defined in none.
        let precise = component(&axis_of("fix verify_token"), "resolution");
        let vague = component(&axis_of("fix the login handler"), "resolution");
        assert!(
            vague < precise / 2.0,
            "vague {vague} should be far below precise {precise}"
        );
    }

    #[test]
    fn a_name_that_does_not_exist_here_resolves_to_nothing() {
        let axis = axis_of("fix compute_paycheck");
        assert_eq!(component(&axis, "resolution"), 0.0);
        let detail = &axis.components[0].detail;
        assert!(detail.contains("not in this repository"), "got {detail}");
    }

    #[test]
    fn ubiquitous_prose_words_are_not_treated_as_names() {
        // "the" is in 98 of 100 files; counting it as a referent would drag
        // every prompt down regardless of how precise its real names are.
        let corpus = repo();
        let found = referents(&tokenize("fix the verify_token"), &[], &corpus);
        assert!(
            !found.iter().any(|r| r.text == "the"),
            "got {:?}",
            found.iter().map(|r| &r.text).collect::<Vec<_>>()
        );
        assert!(found.iter().any(|r| r.text == "verify_token"));
    }

    #[test]
    fn paths_count_as_referents() {
        let corpus = repo();
        let found = referents(&tokenize("edit src/auth/login.rs"), &[], &corpus);
        assert!(found.iter().any(|r| r.text == "src/auth/login.rs"));
    }

    #[test]
    fn specificity_rewards_rare_names_over_common_words() {
        let rare = component(&axis_of("verify_token"), "specificity");
        let common = component(&axis_of("the handler"), "specificity");
        assert!(rare > common, "rare {rare} vs common {common}");
    }

    #[test]
    fn a_deictic_with_nothing_before_it_dangles() {
        let bare = component(&axis_of("fix it"), "deixis");
        let anchored = component(&axis_of("verify_token panics, fix it"), "deixis");
        assert!(bare < anchored, "bare {bare} vs anchored {anchored}");
        assert_eq!(anchored, g::DEIXIS_MAX);
    }

    #[test]
    fn every_deictic_dangles_when_the_prompt_names_nothing() {
        assert_eq!(dangling_deixis(&[0, 5, 9], &[]), 3);
    }

    #[test]
    fn a_deictic_after_a_referent_is_attached() {
        let found = vec![Referent {
            text: "verify_token".into(),
            offset: 4,
            weight: 1.0,
            candidates: Some(1),
        }];
        assert_eq!(dangling_deixis(&[20], &found), 0);
        assert_eq!(dangling_deixis(&[1], &found), 1);
    }

    #[test]
    fn the_axis_never_exceeds_its_maximum() {
        for text in [
            "",
            "it",
            "verify_token verify_token verify_token",
            "src/auth/login.rs verify_token parse_args",
            &"verify_token ".repeat(300),
        ] {
            let axis = axis_of(text);
            assert!(
                axis.earned <= axis.max + 1e-9 && axis.earned >= 0.0,
                "{text:?} scored {}",
                axis.earned
            );
        }
    }

    #[test]
    fn resolution_falls_off_logarithmically() {
        let at = |n: u32| {
            Referent {
                text: "x".into(),
                offset: 0,
                weight: 1.0,
                candidates: Some(n),
            }
            .resolution()
        };
        assert_eq!(at(1), 1.0);
        assert!(at(2) > at(4) && at(4) > at(37));
        // The step from 1 to 2 must dwarf the step from 30 to 40.
        assert!((at(1) - at(2)) > (at(30) - at(40)) * 5.0);
    }
}
