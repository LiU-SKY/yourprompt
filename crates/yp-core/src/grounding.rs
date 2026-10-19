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
use crate::corpus::{Corpus, TermFacts};
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
    /// clearly meant as names. False for a prose word that turned out to name
    /// something the repository has: a definition, or a module file.
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
/// Explicit names -- code spans, paths, identifiers, a word called like a
/// function, the parts of a qualified name -- always count. A plain prose word
/// counts only if it is neither grammar nor syntax, the repository contains
/// it without using it everywhere, and something there is called that: a
/// definition, or a module file.
///
/// `text` is the instruction the tokens were cut from.
pub fn referents(
    text: &str,
    tokens: &[Token],
    instruction_words: &[Span],
    corpus: &dyn Corpus,
) -> Vec<Referent> {
    let claimed = Claimed::new(instruction_words);
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];
        // A pasted snippet is the most groundable thing a prompt can contain
        // -- it is made almost entirely of names from the codebase. Treating
        // the whole block as one referent, as this used to, meant looking up
        // an entire multi-line snippet as if it were an identifier, never
        // resolving it, and throwing away the best evidence in the prompt. So
        // the block is re-read and its own names are collected instead.
        //
        // Code is still excluded from the *smell* matcher, which is a
        // different question: code is not vague prose.
        if token.kind == TokenKind::CodeSpan {
            let region = token.span.slice(text);
            // The stand-in for a cut-out attachment names nothing. Read as
            // code it was looked up as "attached": missing from the demo's
            // sample, where it cost the showcase prompt with a file pasted
            // under it resolution, and defined once in django, where it gave
            // a SWE-bench issue carrying two pastes all 115.7 of its
            // specificity points.
            if region != crate::params::prompt::ATTACHMENT_MARKER {
                for inner in crate::tokenize_snippet(crate::prompt::strip_fence(region)) {
                    // Inside a snippet a bare lowercase word *is* a name --
                    // `ccode`, `sinc` -- even though the tokenizer calls it a
                    // Word because it carries no underscore or camel hump.
                    // Language syntax is the exception: `import` and `return`
                    // name nothing anyone could be pointing at.
                    if yp_lang::is_code_keyword(&inner.text) {
                        continue;
                    }
                    push_referent(&mut out, &inner, token.span.start, &claimed, corpus, true);
                }
            }
            i += 1;
            continue;
        }

        // `AuthError::Expired` is two words to the tokenizer and one name to
        // whoever typed it. The parts after the first are read through the
        // ones before them, so `Expired` is the one in `AuthError` rather than
        // whichever of its namesakes the repository happens to hold.
        if let Some(chain) = qualified_name(tokens, i) {
            push_qualified(&mut out, text, tokens, &chain, corpus);
            i = chain[chain.len() - 1] + 1;
            continue;
        }

        // `gravity()` is a function the user means, however plain the word.
        // As prose it was looked up only when the repository already knew it,
        // so an invented one passed in silence and a real one too common to
        // be a name -- `login()` in the demo's sample -- was never counted.
        if token.kind == TokenKind::Word && called(tokens, i) {
            if !yp_lang::is_code_keyword(&token.text) {
                out.push(Referent {
                    text: token.span.slice(text).to_string(),
                    offset: token.span.start,
                    weight: g::EXPLICIT_WEIGHT,
                    explicit: true,
                    facts: corpus.lookup(&token.text),
                });
            }
        } else {
            push_referent(&mut out, token, token.span.start, &claimed, corpus, false);
        }
        i += 1;
    }
    out
}

/// True when `tokens[i]` is followed directly by `()`.
///
/// Empty parentheses only. `Err(AuthError::Expired)` wraps a value rather
/// than naming a function, and prose writes "file(s)" and "test(s)".
fn called(tokens: &[Token], i: usize) -> bool {
    let (Some(open), Some(close)) = (tokens.get(i + 1), tokens.get(i + 2)) else {
        return false;
    };
    open.text == "("
        && close.text == ")"
        && tokens[i].span.end == open.span.start
        && open.span.end == close.span.start
}

/// The token indices of a name qualified with `::`, if one starts at `i`.
fn qualified_name(tokens: &[Token], i: usize) -> Option<Vec<usize>> {
    // A dotted identifier or a path names a file or a module attribute; what
    // follows `::` after one, as in `tests/auth.rs::expired`, is a test in a
    // file rather than a member of a type.
    let segment = |t: &Token| {
        t.kind == TokenKind::Word || (t.kind == TokenKind::Ident && !t.text.contains('.'))
    };
    if !segment(&tokens[i]) {
        return None;
    }
    let mut chain = vec![i];
    let mut at = i;
    while let (Some(a), Some(b), Some(next)) =
        (tokens.get(at + 1), tokens.get(at + 2), tokens.get(at + 3))
    {
        let joined = a.text == ":"
            && b.text == ":"
            && tokens[at].span.end == a.span.start
            && a.span.end == b.span.start
            && b.span.end == next.span.start;
        if !joined || !segment(next) {
            break;
        }
        chain.push(at + 3);
        at += 3;
    }
    (chain.len() > 1).then_some(chain)
}

/// Record every part of a qualified name.
///
/// The first part is looked up on its own. Each later part can denote no more
/// things than the part before it does, since a type has one member of a
/// given name, and nothing at all when either is missing here: no
/// `MoonError::Full` exists where `MoonError` does not.
fn push_qualified(
    out: &mut Vec<Referent>,
    text: &str,
    tokens: &[Token],
    chain: &[usize],
    corpus: &dyn Corpus,
) {
    let start = tokens[chain[0]].span.start;
    let mut prefix: Option<Option<TermFacts>> = None;
    for &k in chain {
        let token = &tokens[k];
        // `crate::`, `self::` and `Self::new()` qualify by syntax, not by
        // anything this repository defines.
        if yp_lang::is_code_keyword(&token.text) {
            continue;
        }
        let own = corpus.lookup(&token.text);
        let facts = match prefix {
            None => own,
            Some(before) => narrow(before, own),
        };
        prefix = Some(facts);
        out.push(Referent {
            text: text[start..token.span.end].to_string(),
            offset: token.span.start,
            weight: g::EXPLICIT_WEIGHT,
            explicit: true,
            facts,
        });
    }
}

/// What a name qualified by another can denote.
fn narrow(qualifier: Option<TermFacts>, member: Option<TermFacts>) -> Option<TermFacts> {
    let (qualifier, member) = (qualifier?, member?);
    Some(TermFacts {
        df: qualifier.df.min(member.df),
        cf: qualifier.cf.min(member.cf),
        def: qualifier.candidates().min(member.candidates()),
    })
}

/// The spans already claimed as instruction words, sorted by start with the
/// furthest end reached so far, so a token is checked against all of them in
/// logarithmic time.
///
/// Checking every token against every span is quadratic in the prompt. With
/// the input uncapped and the question, negation and tokenizer passes made
/// linear, a megabyte of "don't change it." took the hook 7.2 s that way
/// against django's index and 232 ms this way; at the 64 KB cap, 55 ms and
/// 23 ms.
struct Claimed {
    starts: Vec<usize>,
    reach: Vec<usize>,
}

impl Claimed {
    fn new(spans: &[Span]) -> Self {
        let mut sorted = spans.to_vec();
        sorted.sort_unstable_by_key(|s| s.start);
        let mut furthest = 0;
        let reach = sorted
            .iter()
            .map(|s| {
                furthest = furthest.max(s.end);
                furthest
            })
            .collect();
        Claimed {
            starts: sorted.iter().map(|s| s.start).collect(),
            reach,
        }
    }

    /// The same answer as `spans.iter().any(|s| s.overlaps(span))`.
    fn overlaps(&self, span: &Span) -> bool {
        let before = self.starts.partition_point(|&start| start < span.end);
        before > 0 && self.reach[before - 1] > span.start
    }
}

/// Consider one token as a possible referent and record it if it qualifies.
fn push_referent(
    out: &mut Vec<Referent>,
    token: &Token,
    offset: usize,
    claimed: &Claimed,
    corpus: &dyn Corpus,
    in_code: bool,
) {
    let documents = corpus.documents().max(1) as f64;
    let explicit = token.is_referent_candidate() || (in_code && token.kind == TokenKind::Word);
    if !explicit && token.kind != TokenKind::Word {
        return;
    }

    if !explicit && claimed.overlaps(&token.span) {
        // A word already doing duty as an instruction -- "fix", "return",
        // "only" -- is not the user naming something. Repositories are full of
        // the word "fix"; counting it as a referent would let the verb of the
        // sentence drag down the score for the nouns.
        return;
    }

    if !explicit && yp_lang::is_filler(token) {
        // "please" and "thanks" appear in plenty of repositories, but a
        // courtesy is never the thing a prompt is pointing at.
        return;
    }

    if !explicit
        && (yp_lang::legible::is_common_word(&token.text) || yp_lang::is_code_keyword(&token.text))
    {
        // Nor is grammar or syntax. "to", "as" and "at" are in every
        // repository, inside `to_string`, `as_secs` and `expires_at`, and so
        // is the keyword "if", and a small one holds each in a file or two:
        // against the demo's six-file sample, "Add a dark mode toggle to the
        // settings page." took all 150 resolution points for its "to", 597.1
        // where "...in the settings page." scores 447.1.
        return;
    }

    let mut text = token.text.clone();
    let mut facts = corpus.lookup(&token.text);
    if facts.is_none() && token.kind == TokenKind::Path {
        // A file handed over under its bare name: `src/auth/login.rs` is the
        // `login.rs` the corpus holds, as far as anything here can tell.
        if let Some(name) = token.text.rsplit(['/', '\\']).next() {
            if !name.is_empty() && corpus.bare_file(name) {
                facts = corpus.lookup(name);
            }
        }
    }

    if !explicit {
        // Prose. Only interesting if the repository knows the word and does
        // not use it everywhere...
        let Some(known) = facts else { return };
        if known.df as f64 / documents > g::UBIQUITY_CUTOFF {
            return;
        }
        // ...and only when something here is called that. A word nothing
        // defines lives in the repository's identifiers, strings and
        // comments, which makes it vocabulary rather than a name: the
        // sample's `AuthError::Expired`, `expired_token_is_rejected` and
        // "token expired" made "an expired token" a name that "could be any
        // of 2", and `panic!` made "panic" one "defined once". With the rule
        // above, this took the SWE-bench margin of an issue's own repository
        // over the mean of the others from 47.3 to 56.9 points.
        if known.def == 0 {
            let Some((file, module)) = module_file(&token.text, corpus) else {
                return;
            };
            text = file;
            facts = Some(module);
        }
    }

    out.push(Referent {
        text,
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

/// Extensions of the languages in which a module is a file that nothing
/// declares by name. A Rust module is declared with `mod`, and a class or a
/// package declares itself, so those are definitions already.
const MODULE_EXTENSIONS: &[&str] = &["py", "js", "jsx", "ts", "tsx", "c", "h", "cpp"];

/// The source file a prose word names as a module, when nothing in the
/// repository defines the word itself, with what the repository knows of
/// that file.
///
/// In django's index `executor` appears in ten files and is defined in none,
/// while `executor.py` is one file: "the executor" names that file.
fn module_file(word: &str, corpus: &dyn Corpus) -> Option<(String, TermFacts)> {
    MODULE_EXTENSIONS.iter().find_map(|ext| {
        let file = format!("{word}.{ext}");
        let facts = corpus.lookup(&file)?;
        (facts.def > 0).then_some((file, facts))
    })
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
/// The position of the first name the repository knows, if any: the earliest
/// point from which a pronoun has something to point back at.
pub fn first_anchor(referents: &[Referent]) -> Option<usize> {
    referents
        .iter()
        .filter(|r| r.candidates().is_some_and(|c| c > 0))
        .map(|r| r.offset)
        .min()
}

fn dangling_deixis(pronouns: &[usize], referents: &[Referent]) -> usize {
    match first_anchor(referents) {
        None => pronouns.len(),
        Some(first) => pronouns.iter().filter(|&&at| at < first).count(),
    }
}

/// Score axis A.
///
/// `text` is the instruction `tokens` were cut from. `pronoun_offsets` are the
/// positions of vague-pronoun smells found by the clarity pass, reused here
/// rather than detected twice.
pub fn grounding(
    text: &str,
    tokens: &[Token],
    pronoun_offsets: &[usize],
    instruction_words: &[Span],
    attachments: &[&str],
    corpus: &dyn Corpus,
    discount: f64,
) -> (Axis, Vec<Referent>) {
    let found = referents(text, tokens, instruction_words, corpus);

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
                ("score", 90, 900, 2),
                ("session", 30, 100, 12),
            ],
        )
    }

    fn axis_of(text: &str) -> Axis {
        let corpus = repo();
        let tokens = tokenize(text);
        let resources = yp_lang::resources().unwrap();
        let offsets = pronoun_offsets(&resources.smells.find(text));
        grounding(text, &tokens, &offsets, &[], &[], &corpus, 1.0).0
    }

    fn found_in(text: &str, corpus: &dyn Corpus) -> Vec<Referent> {
        referents(text, &tokenize(text), &[], corpus)
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
        // The headline case: `login` is in 37 files and defined in none.
        let precise = component(&axis_of("fix verify_token"), "resolution");
        let vague = component(&axis_of("fix the `login` handler"), "resolution");
        assert!(
            vague > 0.0 && vague < precise / 2.0,
            "vague {vague} should be far below precise {precise}"
        );
        // So is a word defined in twelve.
        let defined = component(&axis_of("fix the session handler"), "resolution");
        assert!(
            defined > 0.0 && defined < precise / 2.0,
            "twelve definitions {defined} vs one {precise}"
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
        // "score" is in 90 of 100 files; counting it as a referent would drag
        // every prompt down regardless of how precise its real names are,
        // although two of those files define it.
        let corpus = repo();
        let found = found_in("fix the score in verify_token", &corpus);
        assert!(
            !found.iter().any(|r| r.text == "score"),
            "got {:?}",
            found.iter().map(|r| &r.text).collect::<Vec<_>>()
        );
        assert!(found.iter().any(|r| r.text == "verify_token"));
        // Named as code, it is still a name.
        assert!(found_in("fix `score`", &corpus)
            .iter()
            .any(|r| r.text == "score"));
    }

    #[test]
    fn paths_count_as_referents() {
        let corpus = repo();
        let found = found_in("edit src/auth/login.rs", &corpus);
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
            let text = "fix verify_token so it returns claims";
            grounding(text, &tokenize(text), &[], &[], &[], corpus, 1.0)
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
            let found = found_in(text, &corpus);
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
            found_in("fix toctree", corpus)
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
        let filler = "the session handler token the session handler token ";
        let anchored = format!("{filler} fix verify_token");
        let unanchored = format!("{filler} fix something");
        assert!(found_in(&unanchored, &corpus)
            .iter()
            .any(|r| r.text == "session"));

        let resolution_of = |text: &str| {
            grounding(text, &tokenize(text), &[], &[], &[], &corpus, 1.0)
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
        let found = found_in("fix zzzz_nothing_here", &corpus);
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
        let found = found_in(
            "this breaks:
`verify_token(claims)`",
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
            grounding(text, &tokenize(text), &[], &[], &[], &corpus, 1.0)
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

    fn named<'a>(found: &'a [Referent], text: &str) -> &'a Referent {
        found.iter().find(|r| r.text == text).unwrap_or_else(|| {
            panic!(
                "{text} is not a referent: {:?}",
                found.iter().map(|r| &r.text).collect::<Vec<_>>()
            )
        })
    }

    #[test]
    fn a_word_called_like_a_function_is_an_explicit_name() {
        // `gravity()` was prose, looked up only to be dropped as unknown, and
        // `login()` was prose too common in the demo's sample to count.
        let corpus = repo();
        let found = found_in("keep gravity() and login() as they are", &corpus);
        let gravity = named(&found, "gravity");
        assert!(gravity.explicit);
        assert_eq!(gravity.candidates(), None);
        let login = named(&found, "login");
        assert!(login.explicit);
        assert_eq!(login.candidates(), Some(37));

        // Without the parentheses, or with something in them, it is prose.
        for text in [
            "keep login (and logout) as it is",
            "keep the login(s) working",
        ] {
            let found = found_in(text, &corpus);
            assert!(found.iter().all(|r| !r.explicit), "{text}");
        }
    }

    #[test]
    fn a_backticked_word_is_an_explicit_name() {
        let corpus = repo();
        let found = found_in("rename `gravity` and `login`", &corpus);
        assert!(found.iter().all(|r| r.explicit));
        assert_eq!(found.iter().filter(|r| r.facts.is_none()).count(), 1);
        assert!(found.iter().any(|r| r.candidates() == Some(37)));
    }

    #[test]
    fn a_qualified_name_is_read_through_its_qualifier() {
        // `Expired` is mentioned in two places, but `AuthError::Expired` is
        // the one in the only `AuthError`.
        let corpus = MapCorpus::new(
            10,
            &[
                ("autherror", 4, 9, 1),
                ("expired", 2, 3, 0),
                ("orbit", 3, 3, 3),
            ],
        );
        let found = found_in("return Err(AuthError::Expired) here", &corpus);
        assert_eq!(named(&found, "AuthError").candidates(), Some(1));
        let member = named(&found, "AuthError::Expired");
        assert!(member.explicit);
        assert_eq!(member.candidates(), Some(1));
        assert!(!found.iter().any(|r| r.text.eq_ignore_ascii_case("err")));

        // A part this repository does not have makes the whole name missing.
        let found = found_in("return MoonError::Full or Orbit::Full", &corpus);
        assert_eq!(named(&found, "MoonError::Full").facts, None);
        assert_eq!(named(&found, "Orbit::Full").facts, None);
        assert_eq!(named(&found, "Orbit").candidates(), Some(3));

        // Language syntax qualifies nothing, and a call keeps its name.
        let found = found_in("call self::orbit() and Self::new()", &corpus);
        assert_eq!(named(&found, "self::orbit").candidates(), Some(3));
        assert!(!found.iter().any(|r| r.text.ends_with("new")));
    }

    #[test]
    fn a_fence_info_string_is_not_a_name() {
        // Pasting the demo's own login.rs in a `rust` fence reported "rust"
        // as not in this repository.
        let corpus = repo();
        for fence in [
            "```rust\nverify_token(token)\n```",
            "```python title=\"auth.py\"\nverify_token(token)\n```",
            "``` c++\nverify_token(token)\n```",
        ] {
            let text = format!("fix this:\n{fence}");
            let found = found_in(&text, &corpus);
            assert_eq!(
                found.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
                ["verify_token", "token"],
                "{fence}"
            );
        }
        // On one line it is an inline span, all of it content.
        let found = found_in("fix ```verify_token```", &corpus);
        assert_eq!(named(&found, "verify_token").candidates(), Some(1));
    }

    #[test]
    fn the_attachment_marker_is_not_a_name() {
        // It was looked up as "attached", which cost every grounded prompt
        // carrying a file some resolution, and in a repository that does
        // contain the word, earned it specificity for markup.
        let corpus = MapCorpus::new(10, &[("attached", 1, 1, 1), ("verify_token", 1, 2, 1)]);
        let text = format!(
            "fix verify_token\n```\n{}```",
            "line of the paste\n".repeat(40)
        );
        let parts = crate::prompt::split(&text);
        assert!(parts.has_attachments());
        let found = found_in(parts.instruction(), &corpus);
        assert_eq!(
            found.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
            ["verify_token"]
        );
    }

    #[test]
    fn a_path_resolves_by_its_file_name_when_the_files_had_no_paths() {
        // A browser hands over a picked file as `login.rs`, never as
        // `src/auth/login.rs`.
        let entries = [("login.rs", 1, 1, 1), ("verify_token", 1, 2, 1)];
        let text = "fix src/auth/login.rs, src\\auth\\login.rs and src/banana.rs";

        let bare = MapCorpus::new(1, &entries).with_bare_names();
        let found = found_in(text, &bare);
        assert_eq!(named(&found, "src/auth/login.rs").candidates(), Some(1));
        assert_eq!(named(&found, "src\\auth\\login.rs").candidates(), Some(1));
        assert_eq!(named(&found, "src/banana.rs").facts, None);

        // A walked repository has the real paths: no src/auth/login.rs there
        // means none at all.
        let walked = MapCorpus::new(1, &entries);
        let found = found_in(text, &walked);
        assert_eq!(named(&found, "src/auth/login.rs").facts, None);
    }

    #[test]
    fn a_path_falls_back_to_its_file_name_only_where_that_file_had_no_path() {
        // The page with a picked `login.rs` and a dropped `tests/auth.rs`.
        let mixed = MapCorpus::new(
            2,
            &[
                ("login.rs", 1, 1, 1),
                ("tests/auth.rs", 1, 1, 1),
                ("auth.rs", 1, 1, 1),
            ],
        )
        .with_bare_file("login.rs");
        let found = found_in(
            "fix src/auth/login.rs, src/auth.rs and tests/auth.rs",
            &mixed,
        );
        assert_eq!(named(&found, "src/auth/login.rs").candidates(), Some(1));
        // The only auth.rs here is in tests/.
        assert_eq!(named(&found, "src/auth.rs").facts, None);
        assert_eq!(named(&found, "tests/auth.rs").candidates(), Some(1));
    }

    /// The demo's six-file sample, as far as these tests need it: each word
    /// with the files it is in, its occurrences and its definitions.
    fn sample() -> MapCorpus {
        MapCorpus::new(
            6,
            &[
                // Parts of `to_string`, `as_secs`, `expires_at` and
                // `split_once`, and the keywords themselves.
                ("to", 1, 1, 0),
                ("as", 1, 1, 0),
                ("at", 1, 2, 0),
                ("once", 1, 1, 0),
                ("if", 1, 2, 0),
                ("return", 1, 1, 0),
                ("match", 1, 1, 0),
                // `fn now()`: a definition, and still an adverb in prose.
                ("now", 1, 3, 1),
                // `AuthError::Expired`, `expired_token_is_rejected` and the
                // string "token expired".
                ("expired", 2, 4, 0),
                // `panic!(...)`.
                ("panic", 1, 1, 0),
                // The `tests` directory.
                ("tests", 1, 1, 0),
                ("session", 3, 13, 1),
                ("verify_token", 3, 6, 1),
            ],
        )
    }

    fn texts(found: &[Referent]) -> Vec<&str> {
        found.iter().map(|r| r.text.as_str()).collect()
    }

    #[test]
    fn grammar_and_syntax_are_never_names() {
        let corpus = sample();
        for text in [
            "Add a dark mode toggle to the settings page.",
            "Make the navbar sticky at the top.",
            "Input: as described. Output: as expected.",
            "Return it now, once, if it can match.",
        ] {
            let found = found_in(text, &corpus);
            assert!(found.is_empty(), "{text}: {:?}", texts(&found));
        }
        // "to" took the full 150 resolution points, so the sentence scored
        // as if it named the thing it changes.
        let axis = |text: &str| grounding(text, &tokenize(text), &[], &[], &[], &corpus, 1.0).0;
        assert_eq!(
            component(
                &axis("Add a dark mode toggle to the settings page."),
                "resolution"
            ),
            component(
                &axis("Add a dark mode toggle in the settings page."),
                "resolution"
            ),
        );
        // Written as code, the same words are names.
        let found = found_in("call `now`, then now() and `if`", &corpus);
        assert_eq!(texts(&found), ["now", "now"]);
        assert!(found.iter().all(|r| r.explicit));
    }

    #[test]
    fn a_prose_word_names_something_only_where_something_is_called_that() {
        let corpus = sample();
        let found = found_in(
            "Make verify_token return an error instead of a panic when the \
             token has expired, and keep the session. Tests must pass.",
            &corpus,
        );
        assert_eq!(texts(&found), ["verify_token", "session"]);
        let session = named(&found, "session");
        assert!(!session.explicit);
        assert_eq!(session.candidates(), Some(1));

        // Named as code, a word nothing defines is still looked up.
        let found = found_in("handle `expired` and `panic`", &corpus);
        assert_eq!(named(&found, "expired").candidates(), Some(2));
        assert_eq!(named(&found, "panic").candidates(), Some(1));
    }

    #[test]
    fn a_prose_word_can_name_a_module_file() {
        // django: `executor` is in ten files and defined in none, and
        // `executor.py` is one of them.
        let corpus = MapCorpus::new(
            4694,
            &[
                ("executor", 10, 215, 0),
                ("executor.py", 1, 1, 1),
                ("router", 12, 40, 0),
                ("utils", 30, 90, 0),
                ("utils.js", 2, 2, 2),
                ("utils.ts", 1, 1, 1),
            ],
        );
        let found = found_in("the executor runs before the router", &corpus);
        assert_eq!(texts(&found), ["executor.py"]);
        assert_eq!(found[0].candidates(), Some(1));
        assert!(!found[0].explicit);
        // The first source file of that name found stands for the module.
        let found = found_in("move it to utils", &corpus);
        assert_eq!(texts(&found), ["utils.js"]);
        assert_eq!(found[0].candidates(), Some(2));
    }

    #[test]
    fn claimed_spans_answer_as_a_scan_of_them_would() {
        // Spans overlap, nest, are empty and arrive in any order.
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        for _ in 0..200 {
            let spans: Vec<Span> = (0..next(12))
                .map(|_| {
                    let start = next(60);
                    Span::new(start, start + next(8))
                })
                .collect();
            let claimed = Claimed::new(&spans);
            for start in 0..64 {
                for len in 0..6 {
                    let token = Span::new(start, start + len);
                    assert_eq!(
                        claimed.overlaps(&token),
                        spans.iter().any(|s| s.overlaps(&token)),
                        "{token:?} against {spans:?}"
                    );
                }
            }
        }
    }
}
