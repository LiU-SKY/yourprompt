//! What a pasted block does for the words around it.
//!
//! "fix this" above a fenced stack trace is how people ask Claude Code for a
//! fix, and it scored 278 out of 1000; the same trace pasted without the
//! fence scored 559. The fence is what a careful user adds, and it was
//! costing them. The block became one code token that the smell matcher
//! skips, so the instruction read as "fix this": a dangling pronoun in a
//! prompt three tokens long.
//!
//! A pasted block is something to point at. A deictic pronoun in a prompt
//! that carries one is taken to mean it, wherever it sits relative to the
//! block, so it is neither a clarity smell nor dangling deixis. And the block
//! is part of what the prompt says, so its words count toward the substance
//! clarity is scaled by: a bounded number of them, and only words that read
//! as language. A fence around a mashed keyboard, or around one word held
//! down, gives a pronoun nothing to point at and the prompt nothing to stand
//! on.

use std::collections::HashSet;

use yp_lang::{SmellHit, SmellId, Span, Token, TokenKind};

use crate::params::prompt as p;
use crate::prompt::{heads, Parts};

/// How much a prompt's pasted material has to offer.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pasted {
    /// Distinct words that read as language across the pasted blocks,
    /// discounted by how much of the material is language at all and how
    /// varied it is.
    terms: usize,
}

impl Pasted {
    pub fn of(parts: &Parts<'_>) -> Self {
        Self {
            terms: legible_terms(&parts.pasted()),
        }
    }

    /// Whether there is enough here for a pronoun to point at.
    pub fn is_material(&self) -> bool {
        self.terms >= p::PASTED_MIN_TERMS
    }

    /// Content units the material adds to the prompt's substance.
    pub fn substance(&self) -> usize {
        if self.is_material() {
            self.terms.min(p::PASTED_TERMS_MAX)
        } else {
            0
        }
    }

    /// Set aside the deictic pronouns this material gives something to
    /// point at.
    ///
    /// Returns the smell hits that still count, and the spans of the
    /// pronouns taken to mean the material.
    pub fn anchor(&self, hits: Vec<SmellHit>) -> (Vec<SmellHit>, Vec<Span>) {
        if !self.is_material() {
            return (hits, Vec::new());
        }
        let (anchored, kept): (Vec<SmellHit>, Vec<SmellHit>) = hits
            .into_iter()
            .partition(|h| h.id == SmellId::VaguePronoun && yp_lang::is_deictic(&h.term));
        (kept, anchored.into_iter().map(|h| h.span).collect())
    }
}

/// Distinct words in `blocks` that read as language, scaled by the
/// material's own legibility and variety.
///
/// Only words that can be checked count: common English words, the keywords
/// of programming languages, and written Hangul. Identifiers and paths are
/// left out, because without a repository to look them up in `asdf_qwer` is
/// as good a name as `verify_token`, and a fence of underscored mash scored
/// like a stack trace. A trace has its "at", "called" and "value"; code has
/// its `fn`, `let` and `return`.
///
/// The legibility and variety scaling is the one the instruction's absence
/// credit gets, so a fence cannot launder what typing could not: a phrase
/// repeated a hundred times has the variety of one.
///
/// Only the first [`p::PASTED_READ_BYTES`] of material are read.
fn legible_terms(blocks: &[&str]) -> usize {
    let read = heads(blocks, p::PASTED_READ_BYTES);
    if read.is_empty() {
        return 0;
    }
    let tokens: Vec<Token> = read.iter().flat_map(|b| yp_lang::tokenize(b)).collect();
    let mut terms: HashSet<String> = HashSet::new();
    for token in &tokens {
        let reads = match token.kind {
            TokenKind::Word => {
                yp_lang::legible::is_common_word(&token.text)
                    || yp_lang::is_code_keyword(&token.text)
            }
            TokenKind::Hangul => yp_lang::is_legible(token),
            _ => false,
        };
        if reads && !yp_lang::is_filler(token) {
            terms.insert(token.text.to_lowercase());
        }
    }
    let stats = crate::stats::analyze(&read.join("\n"), &tokens);
    (terms.len() as f64 * stats.legibility() * stats.diversity()).floor() as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt::{from_parts, split};

    fn terms(block: &str) -> usize {
        legible_terms(&[block])
    }

    const TRACE: &str = "thread 'main' panicked at src/auth/login.rs:9:41:\n\
                         called `Result::unwrap()` on an `Err` value: ()";

    #[test]
    fn a_stack_trace_is_material() {
        assert!(terms(TRACE) >= p::PASTED_MIN_TERMS, "got {}", terms(TRACE));
        let js = "TypeError: Cannot read properties of undefined (reading 'map')\n    \
                  at UserList (src/components/UserList.tsx:14:22)";
        assert!(terms(js) >= p::PASTED_MIN_TERMS, "got {}", terms(js));
    }

    #[test]
    fn code_is_material() {
        let rust = "fn verify_token(t: &Token) -> Result<Claims, AuthError> {\n    \
                    let claims = t.decode().unwrap();\n    Ok(claims)\n}";
        assert!(terms(rust) >= p::PASTED_MIN_TERMS, "got {}", terms(rust));
        let python = "def verify(token):\n    if token.expired:\n        \
                      raise ExpiredError()\n    return token.claims";
        assert!(
            terms(python) >= p::PASTED_MIN_TERMS,
            "got {}",
            terms(python)
        );
    }

    #[test]
    fn mash_and_held_keys_are_not() {
        for block in [
            "asdfasefawefasf zxdf2wq4rq235wrsadg qwerqwer zxcvzxcv\nasdfasdf qwpeoriu zmxncbv",
            // Shaped like identifiers, which no word list can vouch for.
            "asdf_qwer zxcv_uiop hjkl_bnm qwe_rty asdfQwer zxcvUiop",
            "ㅁㄴㅇㄹ ㅋㅌㅊㅍ ㅂㅈㄷㄱ",
            "아아아아아아 아아아 아아아아",
            &"value ".repeat(40),
            &"error error error\n".repeat(10),
            &"the value of ".repeat(100),
            "1234 5678 9012 3456",
        ] {
            assert!(
                terms(block) < p::PASTED_MIN_TERMS,
                "{block:.30} counted {} terms",
                terms(block)
            );
        }
    }

    #[test]
    fn mash_with_a_few_real_words_in_it_is_not() {
        let block = format!("{} the of and", "qwer asdf zxcv uiop hjkl bnm ".repeat(4));
        assert!(terms(&block) < p::PASTED_MIN_TERMS, "got {}", terms(&block));
    }

    #[test]
    fn a_block_counts_the_same_typed_short_typed_long_or_opened() {
        // A fence too short to cut out stays in the instruction and is pasted
        // material all the same, a long one is cut out as an attachment, and
        // either can arrive as a file opened beside what was typed. All of
        // them must find the same material.
        for repeats in [1, 6] {
            let block: String = (0..repeats)
                .map(|i| format!("{TRACE} (worker {i})\n"))
                .collect();
            let text = format!("fix this\n```\n{block}```");
            let typed = split(&text);
            let opened = from_parts("fix this", &[&block]);
            assert_eq!(typed.has_attachments(), repeats > 1, "{repeats}");
            if repeats == 1 {
                assert!(typed.instruction().contains(TRACE));
            }
            assert!(Pasted::of(&typed).is_material(), "{repeats}");
            assert_eq!(
                Pasted::of(&typed).terms,
                Pasted::of(&opened).terms,
                "{repeats}"
            );
        }
    }

    #[test]
    fn an_inline_span_is_not_pasted_material() {
        let parts = split("fix `verify_token` in `src/auth/login.rs` and `Claims`");
        assert!(parts.pasted().is_empty());
        assert!(!Pasted::of(&parts).is_material());
    }

    #[test]
    fn only_the_start_of_a_long_paste_is_read() {
        let block = format!("{}{}", "가".repeat(3000), "x".repeat(5000));
        let read = heads(&[&block, "never reached"], 8192);
        assert_eq!(read.len(), 1);
        assert!(read[0].len() <= 8192);
        assert!(block.starts_with(read[0]));
        // A cut never splits a character.
        let read = heads(&["가나다"], 4);
        assert_eq!(read, ["가"]);
    }

    #[test]
    fn the_substance_it_adds_is_bounded() {
        let file: String = (0..200)
            .map(|i| format!("fn step_{i}(input: &str) -> bool {{ let value = input.len(); return value > {i}; }}\n"))
            .collect();
        let text = format!("fix this\n```rust\n{file}```");
        assert_eq!(Pasted::of(&split(&text)).substance(), p::PASTED_TERMS_MAX);
    }
}
