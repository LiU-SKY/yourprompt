//! A verb under a prohibition names something not to do.
//!
//! The cue matcher finds words, not clauses. So "Don't change anything. Tests
//! must still pass." scored 673, a B, with "change" as its objective, although
//! the only action it names is the one it forbids: it asks for nothing at all.
//! "dont" without its apostrophe was not even read as a negation, and "asdf
//! asdf src/a.rs must pass tests dont change" scored 627 the same way.
//!
//! A forbidden verb is therefore never the objective, and never counts as a
//! second one. It bounds the work instead. Where the prohibition is a scope cue
//! in its own right ("don't", "never", "must not", "without", "말고") that
//! boundary has been counted already and the verb adds nothing. Where it is not
//! ("no need to update", or a Korean verb that carries its own prohibition, as
//! in "변경하지 말 것") the verb is the constraint, so the same boundary earns
//! the same credit however it is phrased.
//!
//! Only prohibitions count. A sentence that says what something does not do
//! ("SimpleLazyObject doesn't implement __radd__", "we never test this with
//! discrete features") names the missing behaviour, which is the request.
//! Reading those as prohibitions cost one SWE-bench issue 273 points.
//!
//! Only the axes and the marks read the settled hits. A forbidden verb is still
//! an instruction word rather than a name, and grounding keeps treating it as
//! one.

use yp_lang::{CueId, Hit, Span};

/// How many words may stand between an English negation and its verb.
const MAX_BRIDGES: usize = 3;

/// Words that may sit between a negation and its verb: "don't ever change",
/// "no need to update", "not to rename". Any other word ends the search,
/// which is what keeps "don't forget to add tests" a request.
const EN_BRIDGES: &[&str] = &[
    "accidentally",
    "also",
    "any",
    "bother",
    "directly",
    "even",
    "ever",
    "further",
    "have",
    "just",
    "manually",
    "need",
    "silently",
    "simply",
    "to",
    "try",
    "unnecessarily",
];

/// Pronouns that make "don't" and "do not" a statement about someone rather
/// than an instruction: "we don't support Windows", "they don't cover the
/// retry path".
const EN_SUBJECTS: &[&str] = &[
    "he", "i", "it", "she", "that", "these", "they", "this", "those", "we", "which", "who", "you",
];

/// Auxiliaries that make "not" report a state: "it does not compile", "this
/// will not build".
const EN_STATES: &[&str] = &[
    "are", "can", "could", "did", "does", "had", "has", "have", "is", "may", "might", "was",
    "were", "will", "would",
];

/// Words that can open an instruction ahead of "never", "no" or "stop": "and
/// never touch", "please stop".
const EN_LEADS: &[&str] = &[
    "also", "and", "but", "just", "now", "or", "please", "pls", "plz", "simply", "so", "still",
    "then",
];

/// Modals that make what follows them a rule: "must not", "should never".
const EN_MODALS: &[&str] = &["must", "need", "shall", "should"];

/// Read every action verb in the light of any prohibition governing it.
///
/// Returns the hits the axes and marks should score: a forbidden verb is
/// dropped, or relabelled as the scope constraint it states when no scope cue
/// counts that constraint already. Every other hit passes through unchanged
/// and in order.
///
/// `hits` must be sorted and non-overlapping, as the cue matcher returns them.
pub(crate) fn settle(text: &str, hits: &[Hit<CueId>]) -> Vec<Hit<CueId>> {
    let mut out = Vec::with_capacity(hits.len());
    // The verb before this one, for "don't change the schema or drop a
    // column", where one prohibition governs both.
    let mut previous: Option<(Span, Option<Span>)> = None;

    for hit in hits {
        if hit.id != CueId::ActionVerb {
            out.push(hit.clone());
            continue;
        }
        let prohibition = korean(text, hit.span)
            .or_else(|| english(text, hit.span))
            .or_else(|| match previous {
                Some((verb, Some(found))) if continues(text, verb, hit.span) => Some(found),
                _ => None,
            });
        previous = Some((hit.span, prohibition));

        match prohibition {
            None => out.push(hit.clone()),
            Some(at) if counted(hits, at) => {}
            Some(_) => out.push(Hit {
                id: CueId::ScopeConstraint,
                term: hit.term.clone(),
                span: hit.span,
            }),
        }
    }
    out
}

/// Whether a scope cue the matcher already found covers this prohibition.
///
/// The matcher returns hits sorted and non-overlapping, so their ends are
/// sorted as well: the hits that overlap the prohibition are the last few
/// that start before it ends. Scanning every hit for every forbidden verb was
/// quadratic: "don't change " 80,000 times took 6.3 s on the demo page.
fn counted(hits: &[Hit<CueId>], prohibition: Span) -> bool {
    let before_end = hits.partition_point(|h| h.span.start < prohibition.end);
    hits[..before_end]
        .iter()
        .rev()
        .take_while(|h| h.span.end > prohibition.start)
        .any(|h| h.id == CueId::ScopeConstraint && h.span.overlaps(&prohibition))
}

/// A Korean verb forbidden by what follows it: "수정하지 마", "변경하지 말
/// 것", "리팩터링은 하지 마", "업데이트할 필요 없어".
///
/// Also catches an English verb with a Korean prohibition, "refactor하지 마",
/// which is how a good share of Korean prompts are actually typed. "-지 않"
/// and "-지 못" describe rather than forbid, and are left alone.
fn korean(text: &str, verb: Span) -> Option<Span> {
    let after = &text[verb.end..];
    // A noun-like verb ("리팩터링", "업데이트") is negated through 하다, often
    // behind a topic or object particle.
    let s = after
        .strip_prefix(['은', '는', '도', '을', '를'])
        .map_or(after, str::trim_start);

    // "업데이트할 필요 없어" is "no need to update".
    if let Some(rest) = s.strip_prefix('할') {
        let rest = rest.trim_start().strip_prefix("필요")?;
        let rest = rest
            .strip_prefix(['는', '가', '도'])
            .unwrap_or(rest)
            .trim_start();
        return rest.starts_with('없').then(|| ending(text, verb, rest));
    }

    let s = s.strip_prefix('하').unwrap_or(s);
    let s = s.strip_prefix('지')?;
    let s = s.strip_prefix(['는', '도']).unwrap_or(s).trim_start();
    // 마 and 말 are both 말다, the auxiliary that forbids: "-지 마", "-지
    // 말고", "-지 말 것", "-지 마세요".
    s.starts_with(['마', '말']).then(|| ending(text, verb, s))
}

/// The span from the end of `verb` through the first character of `rest`,
/// which must be a suffix of `text`.
fn ending(text: &str, verb: Span, rest: &str) -> Span {
    let at = text.len() - rest.len();
    let first = rest.chars().next().map_or(0, char::len_utf8);
    Span::new(verb.end, at + first)
}

/// An English verb forbidden by what comes before it: "don't change", "never
/// touch", "no need to update", "must not rename", "dont change".
fn english(text: &str, verb: Span) -> Option<Span> {
    let mut end = verb.start;
    // The words passed over between the verb and the negation, nearest the
    // verb first.
    let mut bridges: Vec<String> = Vec::new();
    for _ in 0..=MAX_BRIDGES {
        let (start, word) = word_before(text, end)?;
        let folded = fold(word);
        match forbids(text, &folded, start, &bridges) {
            Some(true) => return Some(Span::new(start, start + word.len())),
            Some(false) => return None,
            None => {}
        }
        if !EN_BRIDGES.contains(&folded.as_str()) {
            return None;
        }
        bridges.push(folded);
        end = start;
    }
    None
}

/// Whether the negation `word`, which starts at `at`, forbids the verb it
/// governs. `None` when `word` is not a negation at all.
fn forbids(text: &str, word: &str, at: usize, bridges: &[String]) -> Option<bool> {
    let ahead = lead(text, at);
    let ahead_word = ahead.as_ref().map(|(_, w)| w.as_str());
    let in_list = |list: &[&str]| ahead_word.is_some_and(|w| list.contains(&w));

    let verdict = match word {
        "without" => true,
        // After a noun, "never" reports: "the job never runs" asks for a job
        // that runs.
        "never" => instructs(text, ahead.as_ref()) || in_list(EN_MODALS),
        "no" | "stop" => instructs(text, ahead.as_ref()),
        "not" => match ahead_word {
            // "why not refactor it?" suggests the work; "whether or not to
            // cache" asks about it.
            Some("why" | "or") => return None,
            Some(w) if EN_MODALS.contains(&w) => true,
            // "do not" forbids unless it has a subject: "we do not support".
            Some("do") => {
                let before_do = ahead.as_ref().and_then(|(start, _)| lead(text, *start));
                !states(text, before_do.as_ref())
            }
            // "make sure not to rename it".
            _ if bridges.last().is_some_and(|b| b == "to") => true,
            _ => !states(text, ahead.as_ref()) && !in_list(EN_STATES),
        },
        _ => match contraction(word)? {
            Contraction::Do => !states(text, ahead.as_ref()),
            Contraction::Modal => true,
            Contraction::State => false,
        },
    };
    // "no need to update", "we don't have to rename it": whoever says it,
    // the work is not wanted.
    let no_need = bridges.iter().any(|b| b == "need" || b == "have");
    Some(verdict || no_need)
}

/// Whether the word ahead of a negation makes it a statement about someone:
/// "we don't", "they do not". "make sure you don't" is an instruction all the
/// same.
fn states(text: &str, ahead: Option<&(usize, String)>) -> bool {
    let Some((start, word)) = ahead else {
        return false;
    };
    EN_SUBJECTS.contains(&word.as_str()) && !(word == "you" && urged(text, *start))
}

/// Whether the word ahead of a negation opens an instruction: the start of
/// a clause, a word like "please" or "and", or "make sure you".
fn instructs(text: &str, ahead: Option<&(usize, String)>) -> bool {
    match ahead {
        None => true,
        Some((start, word)) => {
            EN_LEADS.contains(&word.as_str()) || (word == "you" && urged(text, *start))
        }
    }
}

/// Whether the "you" at `at` follows "make sure", "ensure" or "be careful".
fn urged(text: &str, at: usize) -> bool {
    lead(text, at).is_some_and(|(_, w)| matches!(w.as_str(), "sure" | "ensure" | "careful"))
}

/// The three ways a contraction ending in "n't" can bear on a verb.
enum Contraction {
    /// "don't", "dont": an instruction, unless it has a subject.
    Do,
    /// "mustn't", "shouldn't", "needn't": a prohibition, whoever the subject.
    Modal,
    /// "doesn't", "can't", "won't": a statement about what is not so.
    State,
}

/// Classify a contraction, with its apostrophe or, as typed in a hurry,
/// without: "don't", "dont". Only known stems count, so "want" and "print"
/// are not read as "wa-n't" and "pri-n't".
fn contraction(word: &str) -> Option<Contraction> {
    if word == "cannot" {
        return Some(Contraction::State);
    }
    let stem = word
        .strip_suffix("n't")
        .or_else(|| word.strip_suffix("nt"))?;
    match stem {
        "do" => Some(Contraction::Do),
        "must" | "need" | "sha" | "should" => Some(Contraction::Modal),
        "ai" | "are" | "ca" | "could" | "did" | "does" | "had" | "has" | "have" | "is" | "was"
        | "were" | "wo" | "would" => Some(Contraction::State),
        _ => None,
    }
}

/// Lowercase, with a phone's U+2019 apostrophe read as the ASCII one.
fn fold(word: &str) -> String {
    word.to_ascii_lowercase().replace('’', "'")
}

/// The trailing word of `s`, if `s` ends in one, with its start offset.
fn last_word(s: &str) -> Option<(usize, &str)> {
    let start = s
        .char_indices()
        .rev()
        .take_while(|&(_, c)| c.is_ascii_alphabetic() || c == '\'' || c == '’')
        .last()
        .map(|(i, _)| i)?;
    Some((start, &s[start..]))
}

/// The word that ends just before `end`, separated from it by whitespace
/// alone, with its start offset. Punctuation ends a negation's reach.
fn word_before(text: &str, end: usize) -> Option<(usize, &str)> {
    let before = &text[..end];
    let trimmed = before.trim_end();
    if trimmed.len() == before.len() {
        return None;
    }
    last_word(trimmed)
}

/// The word ahead of `at` in the same clause, folded, with its start offset.
/// `None` at the start of a clause: the start of the text, a line break, or
/// punctuation.
fn lead(text: &str, at: usize) -> Option<(usize, String)> {
    let before = text[..at].trim_end_matches([' ', '\t']);
    last_word(before).map(|(start, word)| (start, fold(word)))
}

/// Whether `verb` continues a list the previous verb began under one
/// prohibition: "don't change the schema or drop a column".
fn continues(text: &str, previous: Span, verb: Span) -> bool {
    if previous.end > verb.start {
        return false;
    }
    let gap = &text[previous.end..verb.start];
    let clause_ends = gap.contains([';', '!', '?', '\n']) || gap.contains(". ");
    !clause_ends
        && word_before(text, verb.start)
            .is_some_and(|(_, w)| w.eq_ignore_ascii_case("or") || w.eq_ignore_ascii_case("nor"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn settled(text: &str) -> Vec<(CueId, String)> {
        let hits = yp_lang::resources().unwrap().cue_matcher.find(text);
        settle(text, &hits)
            .into_iter()
            .map(|h| (h.id, h.term))
            .collect()
    }

    fn verbs(text: &str) -> Vec<String> {
        settled(text)
            .into_iter()
            .filter(|(id, _)| *id == CueId::ActionVerb)
            .map(|(_, term)| term)
            .collect()
    }

    fn scope(text: &str) -> Vec<String> {
        settled(text)
            .into_iter()
            .filter(|(id, _)| *id == CueId::ScopeConstraint)
            .map(|(_, term)| term)
            .collect()
    }

    #[test]
    fn a_forbidden_verb_is_not_an_objective() {
        for text in [
            "Don't change anything.",
            "don’t change anything",
            "dont change anything",
            "Do not change anything.",
            "Never change the schema",
            "You must not change the schema",
            "you mustn't change the schema",
            "it shouldn't change the schema",
            "without change to the API",
            "do NOT change the schema",
            "Please don't change the schema",
            "Fix it\nDon't change the schema",
            "fix the parser dont change the schema",
            "asdf asdf src/a.rs must pass tests dont change",
            "make sure you don't change the schema",
            "and never change the schema",
            "you should never change the schema",
        ] {
            assert!(
                !verbs(text).iter().any(|v| v.eq_ignore_ascii_case("change")),
                "{text}: {:?}",
                settled(text)
            );
        }
    }

    #[test]
    fn a_prohibition_that_is_already_a_scope_cue_is_counted_once() {
        // "Don't" bounds the work by itself; "change" under it adds nothing.
        assert_eq!(scope("Don't change the public signature"), ["Don't"]);
        assert_eq!(scope("dont change the public signature"), ["dont"]);
        assert_eq!(scope("never change the public signature"), ["never"]);
    }

    #[test]
    fn a_prohibition_that_is_not_a_scope_cue_makes_the_verb_one() {
        // Nothing else counts these boundaries, so the verb does.
        assert_eq!(scope("no need to update the docs"), ["update"]);
        assert_eq!(scope("there is no need to update the docs"), ["update"]);
        assert_eq!(scope("make sure not to rename the module"), ["rename"]);
        assert_eq!(scope("fix the bug, no refactor"), ["refactor"]);
        assert_eq!(verbs("fix the bug, no refactor"), ["fix"]);
    }

    #[test]
    fn a_statement_of_what_is_missing_keeps_its_verb() {
        // These name the behaviour that is wanted. "doesn't implement" was
        // the whole request in a SWE-bench issue.
        for (text, verb) in [
            ("SimpleLazyObject doesn't implement __radd__", "implement"),
            ("it does not compile on windows", "compile"),
            ("plt.close can't find the figure", "find"),
            ("could not convert string to float", "convert"),
            ("we never test this with discrete features", "test"),
            ("the docs never build on CI", "build"),
            ("they don't cover the retry path", "cover"),
            ("this was never tested, so add a test", "add"),
            ("there is no fix yet", "fix"),
        ] {
            assert!(
                verbs(text).iter().any(|v| v == verb),
                "{text}: {:?}",
                settled(text)
            );
        }
    }

    #[test]
    fn bridging_words_keep_the_negation_but_other_words_end_it() {
        assert!(verbs("don't ever change the schema").is_empty());
        assert!(verbs("never try to fix it by hand").is_empty());
        // "need to" and "have to" say the work is not wanted, whoever the
        // subject is.
        assert!(verbs("we don't need to change the schema").is_empty());
        assert!(verbs("it doesn't have to rename anything").is_empty());
        // "forget" is not a bridge: this is a request to update.
        assert_eq!(verbs("don't forget to update the changelog"), ["update"]);
        assert_eq!(verbs("don't hesitate to refactor"), ["refactor"]);
        // A suggestion is not a prohibition.
        assert_eq!(verbs("why not refactor it into a helper?"), ["refactor"]);
    }

    #[test]
    fn punctuation_ends_a_negation_s_reach() {
        assert_eq!(verbs("No, fix the other one"), ["fix"]);
        assert_eq!(verbs("don't. fix it properly"), ["fix"]);
        assert_eq!(verbs("I said `don't` change"), ["change"]);
    }

    #[test]
    fn one_prohibition_governs_a_list_joined_by_or() {
        assert!(verbs("don't change the schema or drop a column").is_empty());
        assert!(verbs("never add or remove dependencies").is_empty());
        // "and" starts a new request; "or" after a wanted verb is one too.
        assert_eq!(verbs("don't change the schema and add an index"), ["add"]);
        assert_eq!(
            verbs("fix the parser or revert the commit"),
            ["fix", "revert"]
        );
        // A new sentence is a new clause.
        assert_eq!(verbs("don't change it. Or rather, rename it"), ["rename"]);
    }

    #[test]
    fn the_wanted_use_of_the_same_verb_still_counts() {
        let text = "change verify_token to return an error. Don't change login().";
        assert_eq!(verbs(text), ["change"]);
        assert_eq!(scope(text), ["Don't"]);
    }

    #[test]
    fn words_that_merely_end_in_nt_are_not_negations() {
        assert_eq!(verbs("I want to fix the parser"), ["fix"]);
        assert_eq!(verbs("print fix suggestions"), ["print", "fix"]);
    }

    #[test]
    fn a_korean_prohibition_makes_its_verb_a_constraint() {
        for (text, verb) in [
            ("변경하지 말 것", "변경하"),
            ("변경하지 마세요", "변경하"),
            ("수정하지마", "수정하"),
            ("구현하지는 마", "구현하"),
            ("리팩터링은 하지 마", "리팩터링"),
            ("업데이트할 필요 없어", "업데이트"),
            ("refactor하지 마", "refactor"),
        ] {
            assert!(verbs(text).is_empty(), "{text}: {:?}", settled(text));
            assert_eq!(scope(text), [verb], "{text}");
        }
    }

    #[test]
    fn a_korean_prohibition_the_lexicon_knows_is_one_cue() {
        // The matcher reads "수정하지 마" as a single scope cue, longest match
        // first, so there is no verb under it to settle.
        assert_eq!(
            settled("수정하지 마"),
            [(CueId::ScopeConstraint, "수정하지 마".to_string())]
        );
        // "말고" is a scope cue of its own, and counts this boundary already.
        assert_eq!(scope("변경하지 말고 고쳐줘"), ["말고"]);
        assert_eq!(verbs("변경하지 말고 고쳐줘"), ["고쳐"]);
    }

    #[test]
    fn a_korean_statement_of_what_is_missing_keeps_its_verb() {
        // Like "doesn't implement": the missing behaviour is the request.
        assert_eq!(verbs("설정을 변경하지 않고 그냥 넘어가"), ["변경하"]);
        assert_eq!(verbs("수정하지 못했어"), ["수정하"]);
        assert_eq!(verbs("고쳐도 돼"), ["고쳐"]);
    }

    #[test]
    fn hits_other_than_action_verbs_pass_through_in_order() {
        let text = "only in src/cli.rs, refactor parse_args so it returns Config; tests must pass";
        let hits = yp_lang::resources().unwrap().cue_matcher.find(text);
        assert_eq!(settle(text, &hits), hits);
    }

    #[test]
    fn many_prohibitions_cost_about_what_as_many_requests_do() {
        // Every forbidden verb used to look through all the hits for a scope
        // cue that counted its prohibition. At 40,000 of them that took 6.3 s
        // in a debug build, where the same verb asked for 40,000 times took
        // 64 ms. Timed against each other, the two texts measure how the work
        // grows, not how fast the machine is.
        let time = |text: &str| {
            let hits = yp_lang::resources().unwrap().cue_matcher.find(text);
            (0..3)
                .map(|_| {
                    let start = Instant::now();
                    std::hint::black_box(settle(text, &hits));
                    start.elapsed()
                })
                .min()
                .unwrap()
        };
        let wanted = time(&"please change ".repeat(40_000));
        let forbidden = time(&"don't change ".repeat(40_000));
        assert!(
            forbidden < wanted * 20 + Duration::from_millis(100),
            "forbidden {forbidden:?}, wanted {wanted:?}"
        );
    }
}
