use std::collections::HashSet;

use yp_lang::{Token, TokenKind};

/// Structural facts about a prompt, computed once and shared by every axis.
#[derive(Debug, Clone)]
pub struct PromptStats {
    /// Tokens that carry meaning: everything except punctuation.
    pub content_tokens: usize,
    /// How many of those are distinct, for the lexical-variety measure.
    pub distinct_content: usize,
    /// Whether the prompt pastes any code, inline or fenced.
    pub has_code: bool,
    /// Lines that begin a bullet or numbered list item.
    pub list_lines: usize,
    /// Whether the prompt contains anything concrete enough to serve as an
    /// example in its own right: pasted code, a list, a number, or a quoted
    /// literal.
    pub has_concrete: bool,
    /// The share of meaningful tokens that read as language rather than as a
    /// mashed keyboard.
    pub legible_share: f64,
    /// Tokens that could name something in the repository. The grounding axis
    /// resolves these; until it exists they are collected but unused.
    pub referents: Vec<String>,
    /// Content units contributed by pasted material. They count toward
    /// [`Self::substance`] and toward nothing measured over the typed words;
    /// see [`crate::pasted`].
    pub pasted: usize,
    /// Typed tokens that are cue words, or single letters standing in for a
    /// name, beyond what the prompt's other words back. They do not count
    /// toward [`Self::substance`]; see [`Self::judged_tokens`].
    pub unbacked: usize,
}

impl PromptStats {
    /// The same facts, crediting `units` of pasted material toward substance.
    pub fn with_pasted(self, units: usize) -> Self {
        Self {
            pasted: units,
            ..self
        }
    }

    /// The same facts, with `count` of the typed tokens cue words that
    /// nothing backs.
    pub fn with_unbacked(self, count: usize) -> Self {
        Self {
            unbacked: count,
            ..self
        }
    }

    /// The typed tokens there are to judge: every content token but the cue
    /// words nothing backs.
    ///
    /// Clarity rewards the absence of vague words, and a cue word is never
    /// one. Counted in full, cue words were the eight tokens that opened all
    /// 250 points to "Refactor `x`. Returns json, input schema. Tests pass,
    /// assert, verify. Only, keep, unchanged.", which scored 909.8.
    pub fn judged_tokens(&self) -> usize {
        self.content_tokens.saturating_sub(self.unbacked)
    }

    /// How far there is enough here to judge at all: the typed tokens in
    /// [`Self::judged_tokens`] and pasted material together, against the
    /// floor in [`crate::params::substance`].
    pub fn substance(&self) -> f64 {
        crate::params::substance(self.judged_tokens() + self.pasted)
    }

    /// How far this text should be credited for containing no defects.
    ///
    /// Text that is not language contains no ambiguity smells and no dangling
    /// references, and used to collect full marks for both. Components that
    /// reward an *absence* are scaled by this, so a mashed keyboard earns
    /// nothing for the defects it does not have.
    ///
    /// A ratio rather than a threshold, and a generous one: a prompt only has
    /// to be a quarter ordinary words to count as fully written, which leaves
    /// room for prompts that are mostly identifiers and paths.
    pub fn legibility(&self) -> f64 {
        (self.legible_share / crate::params::clarity::LEGIBLE_SHARE_FULL).min(1.0)
    }

    /// How far this text should be credited as *varied* language.
    ///
    /// One word typed ten times is legible, substantial by token count, and
    /// says one word's worth of anything. Components that reward an absence
    /// of defects are scaled by this as well as by [`Self::legibility`],
    /// because a prompt cannot be clearer than it is informative, and the
    /// information in a query lives in its distinct terms.
    ///
    /// Root TTR rather than a raw distinct ratio, so ordinary repetition in
    /// long prompts is not charged for.
    pub fn diversity(&self) -> f64 {
        (self.root_ttr() / crate::params::clarity::DIVERSITY_FULL_RTTR).clamp(0.0, 1.0)
    }

    /// Root type-token ratio: distinct tokens over the square root of total.
    ///
    /// Plain TTR falls as text gets longer no matter how varied it is, which
    /// would penalise detailed prompts for being detailed. Dividing by the
    /// square root is the standard correction and keeps the measure roughly
    /// flat across lengths.
    pub fn root_ttr(&self) -> f64 {
        if self.content_tokens == 0 {
            return 0.0;
        }
        self.distinct_content as f64 / (self.content_tokens as f64).sqrt()
    }
}

/// True for a line that opens a bullet or numbered list item.
pub(crate) fn is_list_line(line: &str) -> bool {
    item_start(line).is_some()
}

/// Where the item of a bullet or numbered list line starts, just past its
/// marker, or `None` for a line that is not one.
fn item_start(line: &str) -> Option<usize> {
    let lead = line.len() - line.trim_start().len();
    let t = &line[lead..];
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = t.strip_prefix(marker) {
            return (!rest.trim().is_empty()).then_some(lead + marker.len());
        }
    }
    // "1. ", "2) ", "10. "
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 3 {
        return None;
    }
    let rest = &t[digits..];
    (matches!(rest.chars().next(), Some('.') | Some(')'))
        && rest.len() > 1
        && rest[1..].starts_with(' ')
        && !rest[1..].trim().is_empty())
    .then_some(lead + digits + 1)
}

/// One bullet or numbered list item: where its marker starts, where its
/// first word starts, and where its line ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ListItem {
    pub marker: usize,
    pub start: usize,
    pub end: usize,
}

/// The list items of `text`, in order.
pub(crate) fn list_items(text: &str) -> Vec<ListItem> {
    let mut items = Vec::new();
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        if let Some(start) = item_start(body) {
            let lead = body.len() - body.trim_start().len();
            let first = body[start..].len() - body[start..].trim_start().len();
            items.push(ListItem {
                marker: offset + lead,
                start: offset + start + first,
                end: offset + body.len(),
            });
        }
        offset += line.len();
    }
    items
}

/// A single letter or syllable typed as a word, the way list items are
/// labelled: a, b, c or 가, 나, 다.
pub(crate) fn is_label(token: &Token) -> bool {
    let mut chars = token.text.chars();
    matches!(token.kind, TokenKind::Word | TokenKind::Hangul)
        && matches!((chars.next(), chars.next()), (Some(c), None) if c.is_alphabetic())
}

/// The lines that open a list item with something in it.
///
/// An item that is only a label, "- a" or "- 나", is not one: the greedy
/// search that found eight cue words worth an A+ went on to add "\n- a",
/// "\n- b" and "\n- c" for the list structure they earned. An item of
/// punctuation alone still counts, as it always has.
fn list_lines(text: &str, tokens: &[Token]) -> usize {
    list_items(text)
        .into_iter()
        .filter(|item| {
            let first = tokens.partition_point(|t| t.span.start < item.start);
            let mut words = tokens[first..]
                .iter()
                .take_while(|t| t.span.start < item.end)
                .filter(|t| t.kind != TokenKind::Punct)
                .peekable();
            words.peek().is_none() || words.any(|t| !is_label(t))
        })
        .count()
}

pub fn analyze(text: &str, tokens: &[Token]) -> PromptStats {
    let mut distinct: HashSet<&str> = HashSet::new();
    let mut content_tokens = 0usize;
    let mut has_code = false;
    let mut referents = Vec::new();

    for token in tokens {
        match token.kind {
            TokenKind::Punct => continue,
            TokenKind::CodeSpan => has_code = true,
            _ => {}
        }
        // Interjections are language but not content: a prompt made of "음...
        // 어..." has not said anything to score. They are skipped here the way
        // punctuation is, so they neither add substance nor dilute variety.
        if yp_lang::is_filler(token) {
            continue;
        }
        content_tokens += 1;
        distinct.insert(token.text.as_str());
        if token.is_referent_candidate() {
            referents.push(token.text.clone());
        }
    }

    let list_lines = list_lines(text, tokens);
    let has_number = tokens.iter().any(|t| t.kind == TokenKind::Number);
    let has_quote = text.contains('"') || text.contains('\'');
    let has_concrete = has_code || list_lines > 0 || has_number || has_quote;
    let legible_share = yp_lang::legible_share(tokens);

    PromptStats {
        content_tokens,
        distinct_content: distinct.len(),
        has_code,
        list_lines,
        has_concrete,
        legible_share,
        referents,
        pasted: 0,
        unbacked: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yp_lang::tokenize;

    fn stats(text: &str) -> PromptStats {
        analyze(text, &tokenize(text))
    }

    #[test]
    fn punctuation_is_not_content() {
        let s = stats("fix it, now!");
        assert_eq!(s.content_tokens, 3);
    }

    #[test]
    fn detects_pasted_code() {
        assert!(stats("run `cargo test` first").has_code);
        assert!(!stats("run cargo test first").has_code);
    }

    #[test]
    fn counts_bullet_and_numbered_lists() {
        let text = "do these:\n- first thing\n- second thing\n1. third\n2) fourth\nnot a list";
        assert_eq!(stats(text).list_lines, 4);
    }

    #[test]
    fn a_bare_dash_or_number_is_not_a_list() {
        assert_eq!(stats("- \n1.\n42 items\n-5 degrees").list_lines, 0);
    }

    #[test]
    fn concreteness_needs_more_than_prose() {
        assert!(!stats("make it a bit nicer please").has_concrete);
        assert!(stats("return `Config`").has_concrete);
        assert!(stats("retry 3 times").has_concrete);
        assert!(
            stats(
                "- first
- second"
            )
            .has_concrete
        );
        assert!(stats("rename it to \"parse_args\"").has_concrete);
    }

    #[test]
    fn root_ttr_penalises_repetition_but_not_length() {
        let varied = stats("refactor the parser so it returns a typed config value");
        let repetitive = stats("fix fix fix fix fix fix fix fix fix fix");
        assert!(
            varied.root_ttr() > repetitive.root_ttr(),
            "varied {} vs repetitive {}",
            varied.root_ttr(),
            repetitive.root_ttr()
        );
    }

    #[test]
    fn root_ttr_of_empty_text_is_zero() {
        assert_eq!(stats("").root_ttr(), 0.0);
    }

    #[test]
    fn collects_identifier_and_path_referents() {
        let s = stats("fix verify_token in src/auth/login.rs");
        assert!(s.referents.iter().any(|r| r == "verify_token"));
        assert!(s.referents.iter().any(|r| r == "src/auth/login.rs"));
        assert!(!s.referents.iter().any(|r| r == "fix"));
    }

    #[test]
    fn a_list_item_that_is_only_a_label_is_not_a_list_line() {
        // "\n- a", "\n- b" were list structure to the greedy search, a few
        // points each.
        assert_eq!(stats("fix it\n- a\n- b\n- 가\n- 나\n1. c").list_lines, 0);
        assert_eq!(
            stats("fix it\n- a test\n- 나머지\n- `x`\n- ->").list_lines,
            4
        );
    }

    #[test]
    fn list_items_know_their_marker_and_their_first_word() {
        let text = "fix it\n  - first\n12. second";
        let items = list_items(text);
        assert_eq!(items.len(), 2);
        assert_eq!(&text[items[0].marker..items[0].start], "- ");
        assert_eq!(&text[items[0].start..items[0].end], "first");
        assert_eq!(&text[items[1].marker..items[1].start], "12. ");
        assert_eq!(&text[items[1].start..items[1].end], "second");
    }

    #[test]
    fn cue_words_nothing_backs_are_not_judged() {
        let s = stats("Refactor `x`. Tests pass.");
        assert_eq!(s.judged_tokens(), 4);
        assert_eq!(s.clone().with_unbacked(2).judged_tokens(), 2);
        assert!(s.clone().with_unbacked(2).substance() < s.substance());
        // Pasted material still counts in full.
        assert_eq!(s.with_unbacked(9).with_pasted(4).substance(), 0.5);
    }
}
