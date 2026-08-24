//! Language resources for `yourprompt`: tokenizer, bilingual smell lexicons,
//! and the dictionary matcher that finds smell occurrences in a prompt.
//!
//! This crate holds everything language-specific so that `yp-core` can stay a
//! pure scoring engine. Adding a language means adding a TOML file here, not
//! touching the scorer.

pub mod lexicon;
pub mod matcher;
pub mod span;
pub mod token;

pub use lexicon::{Lexicon, SmellId};
pub use matcher::{Matcher, SmellHit};
pub use span::Span;
pub use token::{tokenize, Token, TokenKind};

use once_cell::sync::OnceCell;

static BUNDLED: OnceCell<Option<(Lexicon, Matcher)>> = OnceCell::new();

/// The process-wide lexicon and matcher, built from the bundled TOML on first
/// use.
///
/// Returns `None` if the bundled data somehow fails to load. Callers degrade
/// instead of panicking: the hook runs on every prompt the user types and must
/// never be the reason a prompt fails to send.
pub fn bundled() -> Option<&'static (Lexicon, Matcher)> {
    BUNDLED
        .get_or_init(|| {
            let lex = lexicon::load_bundled().ok()?;
            let matcher = Matcher::new(&lex).ok()?;
            Some((lex, matcher))
        })
        .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_resources_are_available() {
        let (lex, matcher) = bundled().expect("bundled resources must load");
        assert!(!lex.terms.is_empty());
        assert!(!matcher.find("just make it nicer").is_empty());
    }

    #[test]
    fn bundled_is_built_once() {
        let a = bundled().unwrap();
        let b = bundled().unwrap();
        assert!(std::ptr::eq(a, b));
    }
}
