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
    /// True for backticked code, paths and identifiers -- things the user
    /// clearly meant as names. False for a prose word that turned out to
    /// exist in the repository.
    pub explicit: bool,
    /// What the repository knows about it. `None` when it has never heard of
    /// it at all.
    pub facts: Option<crate::corpus::TermFacts>,
}

impl Referent {
    /// How many things in the repository this could denote.
    pub fn candidates(&self) -> Option<u32> {
        self.facts.map(|f| f.candidates())
    }

    /// How much knowing this name narrows the work down, in bits.
    ///
    /// Two departures from textbook IDF, both forced by measurement.
    ///
    /// It counts *definition sites*, not raw document frequency. A name that
    /// is a repository's own subject is *mentioned* everywhere in it --
    /// `sphinx` appears in 1286 of sphinx's 1336 files -- so document
    /// frequency says it carries almost no information there, while a
    /// repository that merely imports sphinx a few times makes the same word
    /// look highly distinctive. That inverts the measurement for exactly the
    /// projects whose vocabulary is their subject. Definition sites do not
    /// invert: sphinx defines `toctree` in fifteen places, django in none.
    ///
    /// And it measures against a fixed reference rather than the repository's
    /// own size, because what costs an agent time is the absolute number of
    /// places it must look. Thirty-five candidates is thirty-five files to
    /// read whether the project has a thousand files or five thousand.
    pub fn informativeness(&self, _documents: usize) -> Option<f64> {
        let candidates = self.candidates()?;
        if candidates == 0 {
            return None;
        }
        Some(
            (g::UNINFORMATIVE_CANDIDATES / candidates as f64)
                .log2()
                .max(0.0),
        )
    }

    /// What this name *would* have been worth had it resolved.
    ///
    /// Used to weight the resolution average, so that naming something the
    /// repository has never heard of costs as much as naming something unique
    /// would have gained. Without it an unresolvable name would carry zero
    /// weight and simply vanish from the average.
    pub fn weight_in_resolution(&self, documents: usize) -> f64 {
        self.informativeness(documents)
            .unwrap_or(g::UNINFORMATIVE_CANDIDATES.log2())
            * self.weight
    }
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
        match self.candidates() {
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
    let mut out = Vec::new();
    for token in tokens {
        match token.kind {
            // A pasted snippet is the most groundable thing a prompt can
            // contain -- it is made almost entirely of names from the
            // codebase. Treating the whole block as one referent, as this
            // used to, meant looking up an entire multi-line snippet as if it
            // were an identifier, never resolving it, and throwing away the
            // best evidence in the prompt. So the block is re-read and its
            // own names are collected instead.
            //
            // Code is still excluded from the *smell* matcher, which is a
            // different question: code is not vague prose.
            TokenKind::CodeSpan => {
                for inner in crate::tokenize_snippet(&token.text) {
                    // Inside a snippet a bare lowercase word *is* a name --
                    // `ccode`, `sinc` -- even though the tokenizer calls it a
                    // Word because it carries no underscore or camel hump.
                    // Language syntax is the exception: `import` and `return`
                    // name nothing anyone could be pointing at.
                    if yp_lang::is_code_keyword(&inner.text) {
                        continue;
                    }
                    push_referent(&mut out, &inner, token.span.start, &[], corpus, true);
                }
            }
            _ => push_referent(
                &mut out,
                token,
                token.span.start,
                instruction_words,
                corpus,
                false,
            ),
        }
    }
    out
}

/// Consider one token as a possible referent and record it if it qualifies.
fn push_referent(
    out: &mut Vec<Referent>,
    token: &Token,
    offset: usize,
    instruction_words: &[Span],
    corpus: &dyn Corpus,
    in_code: bool,
) {
    let documents = corpus.documents().max(1) as f64;
    let explicit = token.is_referent_candidate() || (in_code && token.kind == TokenKind::Word);
    if !explicit && token.kind != TokenKind::Word {
        return;
    }

    if !explicit && instruction_words.iter().any(|s| s.overlaps(&token.span)) {
        // A word already doing duty as an instruction -- "fix", "return",
        // "only" -- is not the user naming something. Repositories are full of
        // the word "fix"; counting it as a referent would let the verb of the
        // sentence drag down the score for the nouns.
        return;
    }

    let facts = corpus.lookup(&token.text);

    if !explicit {
        // Prose. Only interesting if the repository knows the word and does
        // not use it everywhere.
        let Some(facts) = facts else { return };
        if facts.df as f64 / documents > g::UBIQUITY_CUTOFF {
            return;
        }
    }

    out.push(Referent {
        text: token.text.clone(),
        offset,
        weight: if explicit {
            g::EXPLICIT_WEIGHT
        } else {
            g::PROSE_WEIGHT
        },
        explicit,
        facts,
    });
}

/// How much of an attachment this repository recognises.
///
/// The share of the distinct names in the attached material that the corpus
/// has heard of at all, or `None` when nothing is attached.
///
/// This is the right question to ask of material, and it is not the question
/// asked of an instruction. An instruction is judged on whether *it* names
/// things precisely. An attachment makes no claims -- it hands over evidence,
/// and what matters is whether that evidence belongs here. Averaging a pasted
/// file's four hundred names into the dozen a person wrote is what made
/// attaching the very file a task was about cost seventy points out of a
/// thousand, and it measured nothing.
///
/// Language syntax is skipped: `let` and `return` live in every codebase and
/// say nothing about which one this is.
pub fn attachment_fit(attachments: &[&str], corpus: &dyn Corpus) -> Option<f64> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut known = 0usize;

    for attachment in attachments {
        for token in yp_lang::tokenize(attachment) {
            if yp_lang::is_code_keyword(&token.text) {
                continue;
            }
            if !(token.is_referent_candidate() || token.kind == TokenKind::Word) {
                continue;
            }
            if !seen.insert(token.text.to_lowercase()) {
                continue;
            }
            if corpus.lookup(&token.text).is_some() {
                known += 1;
            }
        }
    }

    if seen.is_empty() {
        return None;
    }
    Some(known as f64 / seen.len() as f64)
}

/// How much the names in this prompt narrow things down *in this repository*.
///
/// Returns `(mean information per name, share of names the repository knows)`.
/// Only the first is scored; the second is carried for the report.
///
/// Each name contributes its inverse document frequency -- a name confined to
/// one file in five thousand is worth about twelve bits, one spread across
/// half of them close to nothing -- and a name the repository has never heard
/// of contributes zero. Averaged over every name the prompt uses, that is the
/// expected information gained per name mentioned.
///
/// # Why not the Simplified Clarity Score
///
/// SCS was the obvious pick from the pre-retrieval predictor survey and it was
/// the wrong one, in two ways that only showed up under measurement.
///
/// It inverts across collections. Smoothing a term the collection has never
/// seen gives it a large divergence, so scoring a prompt against the *wrong*
/// repository -- where nearly every term is unseen -- reads as maximum
/// specificity. On SWE-bench a sympy issue scored higher specificity against
/// matplotlib than against sympy.
///
/// And it still points the wrong way once restricted to shared terms, because
/// a prompt about sympy uses the words sympy uses constantly, which is *low*
/// divergence. SCS measures how unusual a query is for a collection. The
/// question here is the opposite one: does this prompt name things that belong
/// here.
///
/// Average IDF, from the same survey, asks that directly -- provided unfound
/// names are folded in as zero rather than dropped. Averaging over found names
/// alone reintroduces the same bias from the other side, since a name is
/// commonest exactly where it lives.
pub fn specificity(found: &[Referent], documents: usize) -> (f64, f64) {
    // Explicit names only. A prose word is admitted as a referent *because*
    // the repository contains it, so including prose would make this
    // tautologically high and destroy the discrimination it exists to provide.
    let named: Vec<&Referent> = found.iter().filter(|r| r.explicit).collect();
    if named.is_empty() {
        return (0.0, 0.0);
    }
    let idfs: Vec<f64> = named
        .iter()
        .map(|r| r.informativeness(documents).unwrap_or(0.0))
        .collect();
    let mean_idf = idfs.iter().sum::<f64>() / named.len() as f64;
    let known = idfs.iter().filter(|idf| **idf > 0.0).count();
    (mean_idf, known as f64 / named.len() as f64)
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
        .filter(|r| r.candidates().is_some_and(|c| c > 0))
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
    attachments: &[&str],
    corpus: &dyn Corpus,
    discount: f64,
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
        // Weighted by informativeness, not a flat average. A GitHub issue
        // names a hundred and fifty things, most of them ordinary words that
        // happen to exist in the codebase; averaging them flat means the one
        // name that actually pins the work down is a single percent of the
        // result. Measured on SWE-bench, issues that named a file the fix
        // actually touched scored 1.2 points out of 150 above those that did
        // not -- the signal was there and was being drowned.
        let documents = corpus.documents();
        let weight: f64 = found
            .iter()
            .map(|r| r.weight_in_resolution(documents))
            .sum();
        let earned: f64 = found
            .iter()
            .map(|r| r.weight_in_resolution(documents) * r.resolution())
            .sum();
        let mut ratio = if weight > 0.0 { earned / weight } else { 0.0 };

        // Attached material takes a fixed share of this sub-score, judged on
        // whether the repository recognises it rather than on how precisely it
        // names things. Attaching the right file lifts the score, attaching an
        // unrelated one does not, and attaching anything at all can no longer
        // bury the names the user actually wrote.
        if let Some(fit) = attachment_fit(attachments, corpus) {
            ratio = (1.0 - g::ATTACHMENT_SHARE) * ratio + g::ATTACHMENT_SHARE * fit;
        }

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
            Some(r) => match r.candidates() {
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
    let (mean_idf, coverage) = specificity(&found, corpus.documents());
    let specificity_component = Component::new(
        "specificity",
        g::SPECIFICITY_MAX * (1.0 - (-mean_idf / g::IDF_HALF_LIFE).exp()),
        g::SPECIFICITY_MAX,
        format!(
            "{mean_idf:.1} bits per name, {:.0}% of the names it uses exist in these {} files",
            coverage * 100.0,
            corpus.documents()
        ),
    );

    // ---- deixis ---------------------------------------------------------
    let dangling = dangling_deixis(pronoun_offsets, &found);
    // Scaled by legibility and the caller's discount, for the same reason
    // clarity is: a mashed keyboard has no unattached references because it
    // has no references, one word held down has exactly one, and a greeting
    // refers to nothing because it asks for nothing. Full marks for those
    // absences was the last thing keeping gibberish above the floor.
    let legibility =
        (yp_lang::legible_share(tokens) / crate::params::clarity::LEGIBLE_SHARE_FULL).min(1.0);
    let deixis = Component::new(
        "deixis",
        decay(dangling, g::DEIXIS_MAX, g::DEIXIS_DECAY) * legibility * discount,
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
        vec![resolution, specificity_component, deixis],
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
        grounding(&tokens, &offsets, &[], &[], &corpus, 1.0).0
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
            explicit: true,
            facts: Some(crate::corpus::TermFacts {
                df: 1,
                cf: 3,
                def: 1,
            }),
        }];
        assert_eq!(dangling_deixis(&[20], &found), 0);
        assert_eq!(dangling_deixis(&[1], &found), 1);
    }

    #[test]
    fn specificity_is_higher_against_the_repository_the_prompt_belongs_to() {
        // The regression that left the whole axis at chance on SWE-bench.
        // Under the old SCS formulation a foreign repository -- where nearly
        // every term is unseen -- scored *maximum* divergence, so being
        // further from a codebase read as being more specific.
        let home = MapCorpus::new(50, &[("verify_token", 3, 12, 1), ("claims", 4, 20, 1)]);
        let foreign = MapCorpus::new(50, &[("pyplot", 9, 90, 1), ("colormap", 7, 40, 1)]);

        let axis_of = |corpus: &dyn Corpus| {
            let tokens = tokenize("fix verify_token so it returns claims");
            grounding(&tokens, &[], &[], &[], corpus, 1.0)
                .0
                .components
                .iter()
                .find(|c| c.id == "specificity")
                .unwrap()
                .earned
        };
        assert!(
            axis_of(&home) > axis_of(&foreign),
            "specificity: home {} vs foreign {}",
            axis_of(&home),
            axis_of(&foreign)
        );
    }

    #[test]
    fn a_name_with_one_definition_beats_one_with_many() {
        // Informativeness comes from how many places *define* a name, not
        // how many mention it. Both of these are mentioned equally often.
        let corpus = MapCorpus::new(
            1000,
            &[("single_home", 40, 90, 1), ("many_homes", 40, 90, 30)],
        );
        let mean = |text: &str| {
            let found = referents(&tokenize(text), &[], &corpus);
            specificity(&found, corpus.documents()).0
        };
        assert!(
            mean("fix single_home") > mean("fix many_homes"),
            "one definition {} vs thirty {}",
            mean("fix single_home"),
            mean("fix many_homes")
        );
    }

    #[test]
    fn a_project_is_not_penalised_for_its_own_subject_matter() {
        // The sphinx case. `toctree` is mentioned in 244 of sphinx's 1336
        // files and defined in 15 of them; django mentions it in 35 and
        // defines it nowhere. Judged on document frequency, sphinx looked
        // *less* informed about its own vocabulary than a project that merely
        // uses it, which inverted the measurement for sphinx entirely.
        let home = MapCorpus::new(1336, &[("toctree", 244, 2007, 15)]);
        let user = MapCorpus::new(4694, &[("toctree", 35, 60, 0)]);

        let named = |corpus: &MapCorpus| {
            referents(&tokenize("fix toctree"), &[], corpus)
                .iter()
                .find(|r| r.text == "toctree")
                .and_then(|r| r.informativeness(corpus.documents()))
                .unwrap_or(0.0)
        };
        assert!(
            named(&home) > named(&user),
            "home {} should beat mere user {}",
            named(&home),
            named(&user)
        );
    }

    #[test]
    fn naming_something_that_does_not_exist_counts_against_you() {
        // Otherwise an unresolvable name would carry zero informativeness and
        // drop out of the average instead of costing anything.
        let corpus = repo();
        let absent = Referent {
            text: "compute_paycheck".into(),
            offset: 0,
            weight: 1.0,
            explicit: true,
            facts: None,
        };
        let documents = corpus.documents();
        assert_eq!(absent.informativeness(documents), None);
        assert!(absent.weight_in_resolution(documents) > 0.0);
        assert_eq!(absent.resolution(), 0.0);
    }

    #[test]
    fn one_precise_name_is_not_drowned_by_ordinary_words() {
        // The gold-patch failure: resolution was a flat average, so the single
        // name that pins the work down counted for one part in a hundred and
        // fifty.
        let corpus = repo();
        let filler = "the handler token verify login the handler token verify login ";
        let anchored = format!("{filler} fix verify_token");
        let unanchored = format!("{filler} fix something");

        let resolution_of = |text: &str| {
            grounding(&tokenize(text), &[], &[], &[], &corpus, 1.0)
                .0
                .components
                .iter()
                .find(|c| c.id == "resolution")
                .unwrap()
                .earned
        };
        assert!(
            resolution_of(&anchored) > resolution_of(&unanchored) * 1.5,
            "anchored {} vs unanchored {}",
            resolution_of(&anchored),
            resolution_of(&unanchored)
        );
    }

    #[test]
    fn a_name_nothing_knows_contributes_nothing() {
        let corpus = repo();
        let found = referents(&tokenize("fix zzzz_nothing_here"), &[], &corpus);
        let (mean_idf, coverage) = specificity(&found, corpus.documents());
        assert_eq!(mean_idf, 0.0);
        assert_eq!(coverage, 0.0);
    }

    #[test]
    fn specificity_of_a_prompt_that_names_nothing_is_zero() {
        let corpus = repo();
        assert_eq!(specificity(&[], corpus.documents()), (0.0, 0.0));
    }

    #[test]
    fn names_inside_a_pasted_snippet_are_resolved() {
        // The other regression: a code block was looked up whole, never
        // resolved, and counted as one failed referent -- discarding the most
        // groundable content in the prompt.
        let corpus = repo();
        let found = referents(
            &tokenize(
                "this breaks:
`verify_token(claims)`",
            ),
            &[],
            &corpus,
        );
        assert!(
            found.iter().any(|r| r.text == "verify_token"),
            "got {:?}",
            found.iter().map(|r| &r.text).collect::<Vec<_>>()
        );
        assert!(
            !found.iter().any(|r| r.text.contains('(')),
            "the whole snippet must not be a referent: {:?}",
            found.iter().map(|r| &r.text).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_snippet_full_of_real_names_beats_one_full_of_invented_ones() {
        let corpus = repo();
        let resolution_of = |text: &str| {
            grounding(&tokenize(text), &[], &[], &[], &corpus, 1.0)
                .0
                .components
                .iter()
                .find(|c| c.id == "resolution")
                .unwrap()
                .earned
        };
        assert!(
            resolution_of("`verify_token(x)`") > resolution_of("`compute_paycheck(y)`"),
            "real {} vs invented {}",
            resolution_of("`verify_token(x)`"),
            resolution_of("`compute_paycheck(y)`")
        );
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
                explicit: true,
                // No definition sites, so `candidates` falls back to document
                // frequency -- the ambiguous case this curve exists for.
                facts: Some(crate::corpus::TermFacts {
                    df: n,
                    cf: n,
                    def: 0,
                }),
            }
            .resolution()
        };
        assert_eq!(at(1), 1.0);
        assert!(at(2) > at(4) && at(4) > at(37));
        // The step from 1 to 2 must dwarf the step from 30 to 40.
        assert!((at(1) - at(2)) > (at(30) - at(40)) * 5.0);
    }
}
