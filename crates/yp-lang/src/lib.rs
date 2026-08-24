//! Language resources for `yourprompt`: tokenizer, bilingual lexicons, and the
//! dictionary matcher that finds smell and cue occurrences in a prompt.
//!
//! This crate holds everything language-specific so that `yp-core` can stay a
//! pure scoring engine. Adding a language means adding two TOML files here, not
//! touching the scorer.

pub mod cues;
pub mod lexicon;
pub mod matcher;
pub mod span;
pub mod token;

pub use cues::{CueId, Cues};
pub use lexicon::{Lexicon, SmellId};
pub use matcher::{Hit, Matcher, SmellHit, TermTable};
pub use span::Span;
pub use token::{is_code_keyword, tokenize, Token, TokenKind};

use once_cell::sync::OnceCell;

/// Everything language-specific, compiled once per process.
pub struct Resources {
    pub lexicon: Lexicon,
    pub smells: Matcher<SmellId>,
    pub cues: Cues,
    pub cue_matcher: Matcher<CueId>,
}

static RESOURCES: OnceCell<Option<Resources>> = OnceCell::new();

/// The process-wide language resources, built from the bundled TOML on first
/// use.
///
/// Returns `None` if the bundled data somehow fails to load. Callers degrade
/// instead of panicking: the hook runs on every prompt the user types and must
/// never be the reason a prompt fails to send.
pub fn resources() -> Option<&'static Resources> {
    RESOURCES
        .get_or_init(|| {
            let lexicon = lexicon::load_bundled().ok()?;
            let smells = Matcher::new(&lexicon.table).ok()?;
            let cues = cues::load_bundled().ok()?;
            let cue_matcher = Matcher::new(&cues.table).ok()?;
            Some(Resources {
                lexicon,
                smells,
                cues,
                cue_matcher,
            })
        })
        .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_resources_are_available() {
        let r = resources().expect("bundled resources must load");
        assert!(!r.lexicon.table.is_empty());
        assert!(!r.cues.table.is_empty());
        assert!(!r.smells.find("just make it nicer").is_empty());
        assert!(!r.cue_matcher.find("refactor the parser").is_empty());
    }

    #[test]
    fn resources_are_built_once() {
        let a = resources().unwrap();
        let b = resources().unwrap();
        assert!(std::ptr::eq(a, b));
    }
}
