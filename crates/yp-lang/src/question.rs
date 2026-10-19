//! Is this sentence a direct question?
//!
//! A question asks for something as plainly as an imperative does. "Why does
//! verify_token panic on expired tokens?" wants an explanation the way
//! "explain why verify_token panics" does, but it names no action verb, so it
//! earned no objective and had its clarity cut as text that asks for nothing:
//! 155 out of 1000 for a specific, answerable question.
//!
//! A question is recognised by its shape rather than looked up in the cue
//! lexicon: a sentence that ends in a question mark and
//!
//! - in English, opens with an interrogative ("why", "which") or with the
//!   auxiliary a yes/no question is inverted around ("does", "can"), because
//!   English moves both to the front;
//! - in Korean, contains an interrogative anywhere ("verify_token이 왜 panic
//!   나?"), because Korean leaves it where the answer would go.
//!
//! The question mark is required in both. Without it "I don't know why it
//! fails" is a statement and "왜 자꾸 이러지" is a complaint.

use crate::cues::CueId;
use crate::legible::is_filler;
use crate::matcher::{Hit, KO_FOLLOW};
use crate::span::{covered_by, Span};
use crate::token::{Token, TokenKind};

/// English interrogatives. Sorted, for binary search.
const EN_INTERROGATIVES: &[&str] = &[
    "how", "what", "when", "where", "which", "who", "whom", "whose", "why",
];

/// The auxiliaries an English yes/no question is inverted around. Negative
/// contractions appear as their stems, because the tokenizer splits
/// "doesn't" at the apostrophe. Sorted, for binary search.
const EN_AUXILIARIES: &[&str] = &[
    "am", "are", "aren", "can", "could", "couldn", "did", "didn", "do", "does", "doesn", "don",
    "had", "hadn", "has", "hasn", "have", "haven", "is", "isn", "may", "might", "must", "shall",
    "should", "shouldn", "was", "wasn", "were", "weren", "will", "won", "would", "wouldn",
];

/// Words that may come before the opener without changing what is asked:
/// "so why does it panic?", "and how do I run it?". Courtesies and fillers
/// are skipped as well. Sorted, for binary search.
const EN_LEAD_INS: &[&str] = &["also", "and", "anyway", "but", "now", "so", "then", "well"];

/// Korean interrogatives, matched as stems at the start of a word.
const KO_INTERROGATIVES: &[&str] = &[
    "누가",
    "누구",
    "몇",
    "무슨",
    "무엇",
    "뭐",
    "뭘",
    "어느",
    "어디",
    "어떤",
    "어떻게",
    "언제",
    "얼마",
    "왜",
];

/// Question endings that glue straight onto an interrogative and close the
/// word -- 뭐냐, 뭐예요, 왜죠 -- on top of the particles any Korean stem may
/// carry. Whole endings, so that 왜냐하면 ("because") stays out.
const KO_QUESTION_ENDINGS: &[&str] = &["냐", "냐고", "예요", "죠"];

fn is_question_mark(c: char) -> bool {
    matches!(c, '?' | '？')
}

fn is_terminator(c: char) -> bool {
    matches!(c, '.' | '?' | '!' | '。' | '？' | '！')
}

/// One sentence of the prompt, and whether it ends by asking.
struct Sentence {
    span: Span,
    asks: bool,
}

/// Split `text` into sentences, reading nothing inside code.
///
/// A line break ends a sentence, and so does a run of terminal punctuation
/// that is not followed by more of a name: the dot in "login.rs" or "3.5"
/// ends nothing, and neither does the "?" in a query string.
fn sentences(text: &str, code: &[Span]) -> Vec<Sentence> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if covered_by(code, i) {
            continue;
        }
        if c == '\n' {
            out.push(Sentence {
                span: Span::new(start, i),
                asks: false,
            });
            start = i + 1;
            continue;
        }
        if !is_terminator(c) {
            continue;
        }
        let mut end = i + c.len_utf8();
        let mut asks = is_question_mark(c);
        while let Some(&(j, d)) = chars.peek() {
            if !is_terminator(d) || covered_by(code, j) {
                break;
            }
            asks |= is_question_mark(d);
            end = j + d.len_utf8();
            chars.next();
        }
        let closes = text[end..]
            .chars()
            .next()
            .is_none_or(|n| !(n.is_ascii_alphanumeric() || matches!(n, '_' | '/' | '\\')));
        if closes {
            out.push(Sentence {
                span: Span::new(start, end),
                asks,
            });
            start = end;
        }
    }
    out.push(Sentence {
        span: Span::new(start, text.len()),
        asks: false,
    });
    out
}

/// The opener of an English question, skipping punctuation, courtesies and
/// lead-ins before it, and whether it is an interrogative rather than an
/// auxiliary.
fn english_opener<'t>(words: impl Iterator<Item = &'t Token>) -> Option<(Span, bool)> {
    for token in words {
        let word = token.text.as_str();
        match token.kind {
            TokenKind::Punct => continue,
            TokenKind::Word if is_filler(token) || EN_LEAD_INS.binary_search(&word).is_ok() => {
                continue
            }
            TokenKind::Word if EN_INTERROGATIVES.binary_search(&word).is_ok() => {
                return Some((token.span, true))
            }
            TokenKind::Word if EN_AUXILIARIES.binary_search(&word).is_ok() => {
                return Some((token.span, false))
            }
            _ => return None,
        }
    }
    None
}

/// The first Korean interrogative in the sentence, as the span of its stem.
///
/// The stem must start the word and may only be followed by a particle or a
/// question ending, the same rule the lexicon matcher applies: "왜냐하면" is
/// "because", not "why".
fn korean_interrogative<'t>(words: impl Iterator<Item = &'t Token>) -> Option<Span> {
    for token in words.filter(|t| t.kind == TokenKind::Hangul) {
        for stem in KO_INTERROGATIVES {
            let Some(rest) = token.text.strip_prefix(stem) else {
                continue;
            };
            let bounded = KO_QUESTION_ENDINGS.contains(&rest)
                || rest.chars().next().is_none_or(|c| KO_FOLLOW.contains(c));
            if bounded {
                return Some(Span::new(token.span.start, token.span.start + stem.len()));
            }
        }
    }
    None
}

/// One direct question in a prompt.
#[derive(Debug, Clone, PartialEq)]
pub struct Question {
    /// The word that makes it a question: the English opener or the Korean
    /// interrogative, as a [`CueId::Question`] hit.
    pub hit: Hit<CueId>,
    /// The whole sentence it asks, up to and including the question mark.
    pub sentence: Span,
    /// True when that word is an interrogative ("which", "어떤") rather than
    /// an auxiliary ("could", "does"). An interrogative only ever asks; an
    /// auxiliary keeps its own meaning, and "could" in "could it be a race?"
    /// is still a doubt.
    pub interrogative: bool,
}

/// Every direct question in `text`.
///
/// `tokens` must be [`crate::tokenize`] of the same text. Sentence boundaries
/// are read from the raw text, because the tokenizer drops a sentence-final
/// dot; the words are read from the tokens, so code is never mistaken for a
/// question.
pub fn find(text: &str, tokens: &[Token]) -> Vec<Question> {
    let code: Vec<Span> = tokens
        .iter()
        .filter(|t| t.kind == TokenKind::CodeSpan)
        .map(|t| t.span)
        .collect();
    let mut found = Vec::new();
    for sentence in sentences(text, &code) {
        if !sentence.asks {
            continue;
        }
        // Tokens are in text order, so the words of a sentence are the one
        // run of them that starts inside it, found by binary search. Filtering
        // every token for every sentence was quadratic: 50,000 "? " took 2.3 s
        // in the hook and 6.3 s on the demo page.
        let lo = tokens.partition_point(|t| t.span.start < sentence.span.start);
        let hi = tokens.partition_point(|t| t.span.start < sentence.span.end);
        let words = || tokens[lo..hi].iter();
        let opener = english_opener(words())
            .or_else(|| korean_interrogative(words()).map(|span| (span, true)));
        if let Some((span, interrogative)) = opener {
            found.push(Question {
                hit: Hit {
                    id: CueId::Question,
                    term: span.slice(text).to_string(),
                    span,
                },
                sentence: sentence.span,
                interrogative,
            });
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenize;
    use std::time::{Duration, Instant};

    fn asked(text: &str) -> Vec<String> {
        find(text, &tokenize(text))
            .into_iter()
            .map(|q| q.hit.term)
            .collect()
    }

    #[test]
    fn the_sentence_runs_to_the_question_mark() {
        let text = "verify_token panics. Why does it panic?";
        let found = find(text, &tokenize(text));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].sentence.slice(text).trim(), "Why does it panic?");
    }

    #[test]
    fn interrogatives_are_told_apart_from_auxiliaries() {
        let kinds = |text: &str| -> Vec<bool> {
            find(text, &tokenize(text))
                .into_iter()
                .map(|q| q.interrogative)
                .collect()
        };
        assert_eq!(kinds("Which test fails?"), [true]);
        assert_eq!(kinds("Could it be a race?"), [false]);
        assert_eq!(kinds("어떤 테스트가 실패해?"), [true]);
    }

    #[test]
    fn the_word_lists_are_sorted_so_the_search_works() {
        for list in [EN_INTERROGATIVES, EN_AUXILIARIES, EN_LEAD_INS] {
            let mut sorted = list.to_vec();
            sorted.sort_unstable();
            assert_eq!(list, sorted.as_slice());
        }
    }

    #[test]
    fn an_english_question_opens_with_an_interrogative_or_an_auxiliary() {
        assert_eq!(
            asked("Why does verify_token panic on expired tokens?"),
            ["Why"]
        );
        assert_eq!(
            asked("Does verify_token panic on expired tokens?"),
            ["Does"]
        );
        assert_eq!(asked("which test fails?"), ["which"]);
        assert_eq!(asked("Doesn't login() check the expiry?"), ["Doesn"]);
    }

    #[test]
    fn a_korean_question_carries_its_interrogative_anywhere() {
        assert_eq!(asked("verify_token이 왜 만료된 토큰에서 panic 나?"), ["왜"]);
        assert_eq!(asked("어떤 테스트가 실패해?"), ["어떤"]);
        assert_eq!(asked("이 에러는 뭐야?"), ["뭐"]);
        assert_eq!(asked("이 에러 뭐냐?"), ["뭐"]);
    }

    #[test]
    fn without_a_question_mark_it_is_a_statement() {
        assert!(asked("I don't know why it fails").is_empty());
        assert!(asked("아 진짜 짜증나네 왜 자꾸 이러지 답답해 죽겠다").is_empty());
        assert!(asked("Why does verify_token panic.").is_empty());
    }

    #[test]
    fn a_question_mark_without_an_interrogative_is_not_enough() {
        // A Korean yes/no question is a statement with rising intonation;
        // nothing in the text marks it as one apart from the "?".
        assert!(asked("잘 됐나?").is_empty());
        assert!(asked("verify_token panics on expired tokens?").is_empty());
        // "why" has to open the sentence, not sit inside it.
        assert!(asked("tell me why it fails?").is_empty());
    }

    #[test]
    fn a_stem_inside_a_longer_word_is_not_an_interrogative() {
        // 왜냐하면 is "because".
        assert!(asked("왜냐하면 테스트가 깨졌잖아?").is_empty());
    }

    #[test]
    fn courtesies_and_lead_ins_do_not_hide_the_question() {
        assert_eq!(asked("So why does it panic?"), ["why"]);
        assert_eq!(asked("ok and how do I run it?"), ["how"]);
        assert_eq!(asked("please, can you fix it?"), ["can"]);
    }

    #[test]
    fn each_sentence_is_judged_on_its_own() {
        assert_eq!(
            asked("verify_token panics on expired tokens. Why? How do I stop it?"),
            ["Why", "How"]
        );
        assert_eq!(asked("fix the parser\nwhy does it hang?"), ["why"]);
        // The dot in a file name ends nothing.
        assert_eq!(asked("why does login.rs panic?"), ["why"]);
    }

    #[test]
    fn code_is_never_read_as_a_question() {
        assert!(asked("run `why?` and ```\nwhat does this do?\n```").is_empty());
        assert_eq!(
            asked("```\nlet x = a.b()?;\n```\nwhy does this panic?"),
            ["why"]
        );
    }

    #[test]
    fn a_query_string_is_not_a_sentence_end() {
        assert!(asked("how do I fetch https://example.com/a?b=1 here").is_empty());
    }

    #[test]
    fn many_questions_cost_about_what_as_many_statements_do() {
        // Both texts are 20,000 sentences of one mark each, but only a
        // question has its words searched for an opener. While that search
        // filtered every token, the questions took 4.9 s in a debug build and
        // the exclamations 1.6 ms. Timed against each other, the two measure
        // how the work grows, not how fast the machine is.
        let time = |text: &str| {
            let tokens = tokenize(text);
            (0..3)
                .map(|_| {
                    let start = Instant::now();
                    std::hint::black_box(find(text, &tokens));
                    start.elapsed()
                })
                .min()
                .unwrap()
        };
        let exclamations = time(&"! ".repeat(20_000));
        let questions = time(&"? ".repeat(20_000));
        assert!(
            questions < exclamations * 20 + Duration::from_millis(100),
            "questions took {questions:?}, exclamations {exclamations:?}"
        );
    }
}
