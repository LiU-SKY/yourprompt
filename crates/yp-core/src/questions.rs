//! Direct questions, counted as requests.
//!
//! "Why does verify_token panic on expired tokens?" asks for an explanation
//! as plainly as "explain why verify_token panics" does. It names no action
//! verb, so it scored 155 out of 1000: no objective, and clarity cut to the
//! share kept by text that asks for nothing. A question found by
//! [`yp_lang::question`] now earns the objective an action verb would, and
//! counts as request-shaped.
//!
//! Three exceptions keep that from rewarding the wrong things.
//!
//! An interrogative is not also a hedge. "어떤" is "some" in a statement and
//! "which" in "어떤 테스트가 실패해?". An auxiliary keeps its charge: "could"
//! opens "could it be a race?" and still doubts, and exempting it put "could
//! you fix the bug?" a hundred points over "fix the bug".
//!
//! A question word on its own asks about nothing. "why?" follows something
//! or it is a shrug, and "what? how? why? when? where?" counted as five
//! requests scored 475.5, far above "fix the bug" at 305.4. Questions count
//! only when the prompt says something besides them: words of its own, or
//! pasted material that reads as language.
//!
//! And a question about a pronoun that points at nothing asks about
//! nothing. "Why is it broken again?" holds no name, no code and no pasted
//! output that its "it" could stand for. Neither does the Korean fixture's
//! "그래서 그거 어떻게 됐어?", which, counted as a request, scored 388: above
//! every vague request in the tier over it. "fix it" at least leaves the agent
//! a fix to make; such a question leaves it nothing to look at, so it stands
//! where a remark stands.

use yp_lang::{CueId, Hit, SmellHit, SmellId, Token, TokenKind};

use crate::prompt::is_pasted_block;

/// The direct questions in `text` that count as requests, and `smell_hits`
/// without the interrogatives that asked them.
///
/// `smell_hits` must already be without the pronouns pasted material
/// anchors, so any deictic still among them points at nothing in the prompt.
/// `material` says whether the prompt carries pasted material that reads as
/// language.
pub fn asked(
    text: &str,
    tokens: &[Token],
    smell_hits: Vec<SmellHit>,
    material: bool,
) -> (Vec<Hit<CueId>>, Vec<SmellHit>) {
    let found = yp_lang::question::find(text, tokens);
    let smell_hits: Vec<SmellHit> = smell_hits
        .into_iter()
        .filter(|s| {
            !found.iter().any(|q| {
                q.interrogative && q.hit.span.start <= s.span.start && s.span.end <= q.hit.span.end
            })
        })
        .collect();

    let says_something = material
        || tokens.iter().any(|t| {
            t.kind != TokenKind::Punct
                && !yp_lang::is_filler(t)
                && !is_pasted(text, t)
                && !found
                    .iter()
                    .any(|q| t.span.start <= q.hit.span.start && q.hit.span.end <= t.span.end)
        });
    if !says_something {
        return (Vec::new(), smell_hits);
    }

    let named = names_something(text, tokens);
    let questions = found
        .into_iter()
        .filter(|q| {
            named
                || !smell_hits.iter().any(|s| {
                    s.id == SmellId::VaguePronoun
                        && yp_lang::is_deictic(&s.term)
                        && q.sentence.contains(s.span.start)
                })
        })
        .map(|q| q.hit)
        .collect();
    (questions, smell_hits)
}

/// A pasted block, or the marker left where one was cut out.
fn is_pasted(text: &str, token: &Token) -> bool {
    token.kind == TokenKind::CodeSpan && {
        let raw = token.span.slice(text);
        is_pasted_block(raw) || raw == crate::params::prompt::ATTACHMENT_MARKER
    }
}

/// Whether the typed instruction names anything a pronoun could stand for:
/// an identifier, a path, or inline code.
///
/// A pasted block is not a name. It anchors pronouns by its own rule, which
/// reads what is in it; counting it here would let a fence around anything at
/// all rescue a question.
fn names_something(text: &str, tokens: &[Token]) -> bool {
    tokens.iter().any(|t| match t.kind {
        TokenKind::Ident | TokenKind::Path => true,
        TokenKind::CodeSpan => !is_pasted(text, t),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn questions(text: &str) -> Vec<String> {
        let tokens = yp_lang::tokenize(text);
        let smells = yp_lang::resources().unwrap().smells.find(text);
        asked(text, &tokens, smells, false)
            .0
            .into_iter()
            .map(|h| h.term)
            .collect()
    }

    #[test]
    fn a_question_word_on_its_own_asks_about_nothing() {
        assert!(questions("why?").is_empty());
        assert!(questions("what? how? why? when? where?").is_empty());
        assert!(questions("why? why? why?").is_empty());
        // A fence is not something said unless it reads as language.
        assert!(questions("why?\n```\nasdf qwer zxcv\n```").is_empty());
        // After a statement, the same word asks about the statement.
        assert_eq!(questions("the build fails on CI. Why?"), ["Why"]);
    }

    #[test]
    fn a_question_about_something_counts() {
        assert_eq!(
            questions("Why does verify_token panic on expired tokens?"),
            ["Why"]
        );
        assert_eq!(
            questions("verify_token이 왜 만료된 토큰에서 panic 나?"),
            ["왜"]
        );
        assert_eq!(questions("왜 안 돼?"), ["왜"]);
    }

    #[test]
    fn a_question_about_a_pronoun_that_points_at_nothing_does_not() {
        assert!(questions("why is it broken again?").is_empty());
        assert!(questions("what is this?").is_empty());
        assert!(questions("그래서 그거 어떻게 됐어? 잘 됐나? 괜찮은 거지?").is_empty());
    }

    #[test]
    fn a_name_anywhere_in_the_prompt_gives_the_pronoun_something_to_be() {
        assert_eq!(
            questions("verify_token panics on expired tokens. Why does it do that?"),
            ["Why"]
        );
        assert_eq!(questions("why does it call `unwrap()` here?"), ["why"]);
    }

    #[test]
    fn a_place_is_not_a_pronoun_the_question_is_about() {
        assert_eq!(questions("Is there a test for the parser?"), ["Is"]);
    }

    #[test]
    fn an_interrogative_is_not_also_a_hedge_but_an_auxiliary_is() {
        let smells_after = |text: &str| -> Vec<String> {
            let tokens = yp_lang::tokenize(text);
            let smells = yp_lang::resources().unwrap().smells.find(text);
            asked(text, &tokens, smells, false)
                .1
                .into_iter()
                .map(|s| s.term)
                .collect()
        };
        let before = yp_lang::resources()
            .unwrap()
            .smells
            .find("어떤 테스트가 실패해?");
        assert!(before.iter().any(|s| s.term == "어떤"), "lexicon changed");
        assert!(!smells_after("어떤 테스트가 실패해?").contains(&"어떤".to_string()));
        assert!(smells_after("Could it be a race in verify_token?").contains(&"Could".to_string()));
    }
}
