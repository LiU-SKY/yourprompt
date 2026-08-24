use aho_corasick::{AhoCorasick, MatchKind};

use crate::lexicon::{Lexicon, SmellId};
use crate::span::Span;
use crate::token::{code_regions, is_hangul};

/// Korean characters that may legitimately follow a matched stem.
///
/// Dictionary terms are stems, but Korean glues particles (조사) and auxiliary
/// verb endings onto them: "그거" appears as "그거를", "처리해" as "처리해줘".
/// Matching a stem followed by *any* Hangul would fire on unrelated words --
/// "등" would hit inside "등록", "코드" inside "코드베이스". So a match that is
/// followed by Hangul only counts when that Hangul is one of these, which is a
/// cheap stand-in for a morphological analyser and errs toward false negatives.
const KO_FOLLOW: &str = "은는이가을를에의도만과와랑로으서써부터까지보다처럼한테에게\
                         야여요다임함줘주라봐고며나든지밖뿐대로같하한할했합니";

/// One detected smell occurrence.
#[derive(Debug, Clone, PartialEq)]
pub struct SmellHit {
    pub id: SmellId,
    /// The matched text, exactly as it appears in the prompt.
    pub term: String,
    pub span: Span,
}

/// Multi-pattern dictionary matcher over both bundled lexicons.
///
/// Built once and reused: construction compiles an Aho-Corasick automaton over
/// every term in every language, which is the expensive part. Matching itself
/// is a single linear pass regardless of how many terms the lexicons grow to.
pub struct Matcher {
    ac: AhoCorasick,
    term_category: Vec<SmellId>,
    is_ko: Vec<bool>,
}

impl Matcher {
    pub fn new(lex: &Lexicon) -> Result<Self, Box<dyn std::error::Error>> {
        let ac = AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostLongest)
            .ascii_case_insensitive(true)
            .build(&lex.terms)?;
        let is_ko = lex
            .terms
            .iter()
            .map(|t| t.chars().any(is_hangul))
            .collect();
        Ok(Self {
            ac,
            term_category: lex.term_category.clone(),
            is_ko,
        })
    }

    /// Find every smell occurrence in `text`, skipping code.
    ///
    /// Text inside backticks or fenced blocks is excluded: a pasted snippet
    /// containing `this` or `handle` is code the user is pointing at, not
    /// vague prose they wrote.
    pub fn find(&self, text: &str) -> Vec<SmellHit> {
        let code = code_regions(text);
        let mut hits = Vec::new();

        for m in self.ac.find_iter(text) {
            let span = Span::new(m.start(), m.end());
            if code.iter().any(|r| r.overlaps(&span)) {
                continue;
            }
            let ok = if self.is_ko[m.pattern().as_usize()] {
                ko_boundary_ok(text, &span)
            } else {
                en_boundary_ok(text, &span)
            };
            if !ok {
                continue;
            }
            hits.push(SmellHit {
                id: self.term_category[m.pattern().as_usize()],
                term: span.slice(text).to_string(),
                span,
            });
        }

        hits
    }
}

fn prev_char(text: &str, at: usize) -> Option<char> {
    text[..at].chars().next_back()
}

fn next_char(text: &str, at: usize) -> Option<char> {
    text[at..].chars().next()
}

/// A Latin-script term must sit on word boundaries, so "it" does not fire
/// inside "commit" and "this" does not fire inside "this_thing".
fn en_boundary_ok(text: &str, span: &Span) -> bool {
    let word_char = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let before_ok = prev_char(text, span.start).is_none_or(|c| !word_char(c));
    let after_ok = next_char(text, span.end).is_none_or(|c| !word_char(c));
    before_ok && after_ok
}

/// A Korean stem must not continue a longer word. Nothing Hangul may precede
/// it, and anything Hangul that follows must be a plausible particle or verb
/// ending (see `KO_FOLLOW`).
fn ko_boundary_ok(text: &str, span: &Span) -> bool {
    if prev_char(text, span.start).is_some_and(is_hangul) {
        return false;
    }
    match next_char(text, span.end) {
        Some(c) if is_hangul(c) => KO_FOLLOW.contains(c),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexicon::load_bundled;

    fn matcher() -> Matcher {
        Matcher::new(&load_bundled().unwrap()).unwrap()
    }

    fn ids(text: &str) -> Vec<SmellId> {
        let mut v: Vec<_> = matcher().find(text).into_iter().map(|h| h.id).collect();
        v.sort();
        v.dedup();
        v
    }

    fn terms(text: &str) -> Vec<String> {
        matcher().find(text).into_iter().map(|h| h.term).collect()
    }

    #[test]
    fn catches_english_vagueness() {
        let got = ids("just clean it up a bit, make it nicer if possible");
        assert!(got.contains(&SmellId::VaguePronoun));
        assert!(got.contains(&SmellId::Loophole));
        assert!(got.contains(&SmellId::SubjectiveLanguage));
    }

    #[test]
    fn respects_english_word_boundaries() {
        // "it" inside "commit" and "omit" must not fire.
        assert!(terms("commit the omitted change").is_empty());
    }

    #[test]
    fn catches_korean_vagueness_with_particles() {
        let got = ids("그거를 좀 적당히 고쳐줘");
        assert!(got.contains(&SmellId::VaguePronoun), "got {got:?}");
        assert!(got.contains(&SmellId::AmbiguousAdverb), "got {got:?}");
    }

    #[test]
    fn korean_stem_does_not_fire_inside_a_longer_word() {
        // "등" must not match inside "등록", nor "코드" inside "코드베이스".
        let hits = terms("등록 절차와 코드베이스 구조");
        assert!(
            !hits.iter().any(|t| t == "등" || t == "코드"),
            "got {hits:?}"
        );
    }

    #[test]
    fn korean_stem_fires_when_followed_by_a_particle() {
        // "등" alone would be a false positive inside "등록", but "등등" is a
        // term in its own right and must fire here.
        let hits = terms("로그랑 메트릭 등등 추가해줘");
        assert!(hits.iter().any(|t| t == "등등"), "got {hits:?}");
    }

    #[test]
    fn korean_longest_phrase_wins_over_its_prefix() {
        // "기타" and "등등" are both terms, but so is "기타 등등" -- the
        // longest one must win rather than reporting two overlapping smells.
        let hits = terms("설정 파일, 기타 등등");
        assert!(hits.iter().any(|t| t == "기타 등등"), "got {hits:?}");
        assert!(!hits.iter().any(|t| t == "기타"), "got {hits:?}");
    }

    #[test]
    fn code_spans_are_not_scanned() {
        // `this` and `handle` are inside code, so neither should be reported.
        let hits = terms("run `this.handle(it)` on startup");
        assert!(hits.is_empty(), "got {hits:?}");
    }

    #[test]
    fn longest_term_wins() {
        let hits = terms("do it as fast as possible");
        assert!(
            hits.iter().any(|t| t == "as fast as possible"),
            "got {hits:?}"
        );
    }

    #[test]
    fn matching_is_case_insensitive_for_latin() {
        assert!(!terms("If Possible, refactor").is_empty());
    }

    #[test]
    fn spans_point_at_the_matched_text() {
        let text = "make it better";
        for hit in matcher().find(text) {
            assert_eq!(hit.span.slice(text), hit.term);
        }
    }
}
