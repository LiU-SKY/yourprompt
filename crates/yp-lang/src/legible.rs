//! Is this text language at all?
//!
//! Two of the score's components reward the *absence* of a defect: clarity
//! finds no ambiguity smells, deixis finds no dangling references. Text that
//! is not language has neither, so keyboard mashing collected full marks for
//! both -- `asdfasefawefasf zxdf2wq4rq235wrsadgㅁㄴㅇㄹ` repeated a few times
//! scored 356 out of 1000, with clarity at 250/250.
//!
//! This is the same fault that once gave an empty prompt 464: saying nothing
//! is not the same as saying something well. Emptiness is handled by counting
//! tokens; gibberish needs a way to tell a word from a mash.
//!
//! # How a word is recognised
//!
//! Without shipping a dictionary, three signals do the work:
//!
//! - **Korean is decisive.** Real Korean is written in syllable blocks
//!   (U+AC00–U+D7A3). Bare compatibility jamo -- ㅁㄴㅇㄹ -- is what a Korean
//!   keyboard produces when someone mashes it, and is not a word.
//! - **Code is exempt.** Identifiers, paths and pasted snippets are judged by
//!   the grounding axis against a real repository, which is a far better test
//!   than any word list.
//! - **Latin prose is checked against the commonest few hundred words.** Real
//!   requests are full of them; `asdfasefawefasf` matches none. The check is a
//!   *ratio*, so technical vocabulary the list has never heard of costs
//!   nothing as long as ordinary words hold the sentence together.

use crate::token::{is_hangul, Token, TokenKind};

/// The commonest English words, sorted.
///
/// Not a dictionary and not meant to be one. It only has to answer "does this
/// look like someone writing English", which the function words do on their
/// own -- they are what gibberish never contains.
const COMMON_WORDS: &[&str] = &[
    "a",
    "about",
    "above",
    "add",
    "after",
    "again",
    "against",
    "all",
    "almost",
    "already",
    "also",
    "always",
    "am",
    "an",
    "and",
    "another",
    "any",
    "anything",
    "are",
    "around",
    "as",
    "ask",
    "at",
    "back",
    "bad",
    "be",
    "because",
    "been",
    "before",
    "being",
    "below",
    "best",
    "better",
    "between",
    "both",
    "break",
    "but",
    "by",
    "call",
    "called",
    "can",
    "cannot",
    "case",
    "change",
    "check",
    "code",
    "come",
    "could",
    "current",
    "data",
    "day",
    "default",
    "did",
    "do",
    "document",
    "does",
    "doing",
    "done",
    "down",
    "each",
    "either",
    "else",
    "end",
    "enough",
    "even",
    "ever",
    "every",
    "example",
    "except",
    "expect",
    "expected",
    "far",
    "few",
    "file",
    "files",
    "find",
    "first",
    "fix",
    "following",
    "for",
    "found",
    "from",
    "full",
    "get",
    "give",
    "go",
    "going",
    "good",
    "got",
    "had",
    "handle",
    "has",
    "have",
    "having",
    "he",
    "help",
    "her",
    "here",
    "him",
    "his",
    "how",
    "however",
    "if",
    "in",
    "input",
    "inside",
    "instead",
    "into",
    "is",
    "it",
    "its",
    "just",
    "keep",
    "kind",
    "know",
    "last",
    "later",
    "least",
    "leave",
    "less",
    "let",
    "like",
    "line",
    "list",
    "little",
    "long",
    "look",
    "made",
    "make",
    "makes",
    "making",
    "many",
    "may",
    "me",
    "mean",
    "might",
    "more",
    "most",
    "much",
    "must",
    "my",
    "name",
    "need",
    "never",
    "new",
    "next",
    "no",
    "not",
    "note",
    "nothing",
    "now",
    "number",
    "of",
    "off",
    "often",
    "on",
    "once",
    "one",
    "only",
    "onto",
    "or",
    "order",
    "other",
    "our",
    "out",
    "output",
    "over",
    "own",
    "part",
    "per",
    "place",
    "please",
    "point",
    "possible",
    "problem",
    "put",
    "rather",
    "really",
    "reason",
    "result",
    "right",
    "run",
    "said",
    "same",
    "say",
    "see",
    "seems",
    "set",
    "several",
    "shall",
    "she",
    "should",
    "show",
    "similar",
    "since",
    "so",
    "some",
    "something",
    "sometimes",
    "soon",
    "sort",
    "still",
    "such",
    "sure",
    "take",
    "test",
    "than",
    "that",
    "the",
    "their",
    "them",
    "then",
    "there",
    "these",
    "they",
    "thing",
    "things",
    "think",
    "this",
    "those",
    "though",
    "three",
    "through",
    "time",
    "to",
    "today",
    "together",
    "too",
    "took",
    "try",
    "turn",
    "two",
    "under",
    "until",
    "up",
    "upon",
    "us",
    "use",
    "used",
    "using",
    "usually",
    "value",
    "very",
    "want",
    "was",
    "way",
    "we",
    "well",
    "were",
    "what",
    "when",
    "where",
    "whether",
    "which",
    "while",
    "who",
    "why",
    "will",
    "with",
    "within",
    "without",
    "word",
    "work",
    "working",
    "would",
    "write",
    "wrong",
    "yes",
    "yet",
    "you",
    "your",
];

/// True for a word the list knows. The list is sorted, so this is a binary
/// search.
pub fn is_common_word(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    COMMON_WORDS.binary_search(&lower.as_str()).is_ok()
}

/// True for Hangul that is actually written rather than mashed.
///
/// Syllable blocks are what typing Korean produces. A run of bare
/// compatibility jamo is what a keyboard produces when nobody is typing.
pub fn is_written_hangul(text: &str) -> bool {
    text.chars().any(|c| matches!(c as u32, 0xAC00..=0xD7A3))
}

/// True when this token could plausibly be part of a human request.
pub fn is_legible(token: &Token) -> bool {
    match token.kind {
        // Judged against a real repository by the grounding axis, which is a
        // better test than any word list.
        TokenKind::Ident | TokenKind::Path | TokenKind::CodeSpan | TokenKind::Number => true,
        TokenKind::Hangul => is_written_hangul(&token.text),
        TokenKind::Word => is_common_word(&token.text),
        TokenKind::Punct => true,
    }
}

/// The share of a prompt's meaningful tokens that read as language.
///
/// Punctuation is ignored: it is neither evidence for nor against.
pub fn legible_share(tokens: &[Token]) -> f64 {
    let considered: Vec<&Token> = tokens
        .iter()
        .filter(|t| t.kind != TokenKind::Punct)
        .collect();
    if considered.is_empty() {
        return 0.0;
    }
    let legible = considered.iter().filter(|t| is_legible(t)).count();
    legible as f64 / considered.len() as f64
}

/// True for a character that belongs to a Hangul syllable or jamo.
pub fn is_any_hangul(c: char) -> bool {
    is_hangul(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenize;

    #[test]
    fn the_word_list_is_sorted_so_the_search_works() {
        let mut sorted = COMMON_WORDS.to_vec();
        sorted.sort_unstable();
        assert_eq!(
            COMMON_WORDS,
            sorted.as_slice(),
            "COMMON_WORDS is not sorted"
        );
        assert!(COMMON_WORDS.len() > 200);
    }

    #[test]
    fn ordinary_words_are_recognised_and_mashing_is_not() {
        for word in ["the", "should", "because", "FILE", "Value"] {
            assert!(is_common_word(word), "{word} should be common");
        }
        for word in ["asdfasefawefasf", "zxcvzxcv", "qwerqwer", "wrsadg"] {
            assert!(!is_common_word(word), "{word} should not be common");
        }
    }

    #[test]
    fn written_korean_is_told_apart_from_a_mashed_keyboard() {
        assert!(is_written_hangul("고쳐줘"));
        assert!(is_written_hangul("그거를"));
        // Bare compatibility jamo is what mashing a Korean keyboard produces.
        assert!(!is_written_hangul("ㅁㄴㅇㄹ"));
        assert!(!is_written_hangul("ㅋㅋㅌㅌ"));
    }

    #[test]
    fn a_real_request_reads_as_language() {
        let text = "Rewrite informativeness in crates/yp-core/src/grounding.rs so it \
                    counts definition sites rather than document frequency. Keep the \
                    rest of the file unchanged.";
        assert!(legible_share(&tokenize(text)) > 0.5);
    }

    #[test]
    fn a_korean_request_reads_as_language() {
        let text = "crates/yp-core/src/grounding.rs 의 informativeness 를 다시 작성해줘";
        assert!(legible_share(&tokenize(text)) > 0.5);
    }

    #[test]
    fn keyboard_mashing_does_not() {
        let mash = "asdfasefawefasf zxdf2wq4rq235wrsadg ㅁㄴㅇㄹㅎㅁㄴㅇㄻㄴㅇㄹ \
                    qwerqwer zxcvzxcv asdfasdf";
        assert!(
            legible_share(&tokenize(mash)) < 0.2,
            "got {}",
            legible_share(&tokenize(mash))
        );
    }

    #[test]
    fn technical_vocabulary_costs_nothing_when_the_sentence_holds_together() {
        // None of these nouns are in the list; the sentence around them is.
        let text = "the simplified_clarity_score and the informativeness of a \
                    referent should not both be used here";
        assert!(legible_share(&tokenize(text)) > 0.5);
    }

    #[test]
    fn empty_text_is_not_legible() {
        assert_eq!(legible_share(&tokenize("")), 0.0);
        assert_eq!(legible_share(&tokenize("   ")), 0.0);
    }
}
