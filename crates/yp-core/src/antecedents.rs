//! A pronoun after a name in its own sentence stands for that name.
//!
//! With a repository index, grounding asks whether a pronoun has an
//! antecedent: a name the repository knows, earlier in the prompt. Without
//! one, every pronoun was charged as a clarity smell, the "it" in "change
//! verify_token so it returns Err(AuthError::Expired)" included. That cost the
//! showcase prompt 38.5 of its 250 clarity points, and the page marked the
//! "it" as pointing at nothing on its own best example.
//!
//! So with no index, a pronoun with something shaped like code before it in
//! the same sentence is taken to mean that, the way grounding takes it to
//! mean a name that resolves. Shaped like code is snake_case, CamelCase, a
//! dotted or qualified name (`mod.attr`, `Foo::bar`), a path, a call
//! (`login()`), or an inline code span. Such a pronoun is not a smell, and it
//! is marked as the grounded one is. Whether the name exists is what an index
//! would say, so a made-up name anchors a pronoun as well as a real one, just
//! as it scores as well as one everywhere else without an index.
//!
//! The reach is the sentence, narrower than grounding's look back over the
//! whole prompt, because here nothing confirms that the name is one: in "Fix
//! verify_token. It panics on expired tokens." the "It" is still charged.
//! "fix it", "it's broken again" and "make it better" name nothing and stay
//! charged. A pasted block is not a name: it anchors pronouns by its own rule,
//! which reads what is in it, and counting it here would let a fence around
//! anything rescue them.
//!
//! The pronouns are the ones pasted material may anchor: those that stand for
//! a thing ("it", "this", "they", 이거, 그거). "the same" and 아까 reach back
//! into the conversation, and "there" and 여기 are places.
//!
//! Korean follows the same rule. A Korean deictic after a name in its
//! sentence ("verify_token 그거 왜 이래?") means the name just as the English
//! one does, though prompts written that way are rare: none of the 32 in
//! bench/korean.txt is.

use yp_lang::span::covered_by;
use yp_lang::{SmellHit, SmellId, Span, Token, TokenKind};

use crate::params::prompt::ATTACHMENT_MARKER;
use crate::prompt::is_pasted_block;

/// Abbreviations with a name's shape: "e.g." is dotted like `mod.attr`.
const ABBREVIATIONS: &[&str] = &["e.g", "i.e"];

/// Set aside the pronouns a code-shaped name before them anchors.
///
/// Returns the smell hits that still count, and the spans of the pronouns
/// taken to mean a name. Only for a prompt scored without an index; with one,
/// grounding judges every pronoun against the names that actually resolve.
pub(crate) fn anchor(
    text: &str,
    tokens: &[Token],
    hits: Vec<SmellHit>,
) -> (Vec<SmellHit>, Vec<Span>) {
    let names = names(text, tokens);
    if names.is_empty() {
        return (hits, Vec::new());
    }
    let starts = sentence_starts(text, tokens);
    let (anchored, kept): (Vec<SmellHit>, Vec<SmellHit>) = hits.into_iter().partition(|h| {
        h.id == SmellId::VaguePronoun && yp_lang::is_deictic(&h.term) && {
            let sentence = starts[starts.partition_point(|&s| s <= h.span.start) - 1];
            let first = names.partition_point(|n| n.start < sentence);
            names.get(first).is_some_and(|n| n.end <= h.span.start)
        }
    });
    (kept, anchored.into_iter().map(|h| h.span).collect())
}

/// The spans of the tokens shaped like code, in order.
fn names(text: &str, tokens: &[Token]) -> Vec<Span> {
    tokens
        .iter()
        .filter(|t| match t.kind {
            TokenKind::Ident => !ABBREVIATIONS.contains(&t.text.to_ascii_lowercase().as_str()),
            TokenKind::Path => true,
            TokenKind::CodeSpan => {
                let raw = t.span.slice(text);
                !is_pasted_block(raw) && raw != ATTACHMENT_MARKER
            }
            // A plain word is a name when it is called, `login()`, though not
            // when it is a plural, "file(s)"; or when it is qualified,
            // `Foo::bar`.
            TokenKind::Word => {
                let after = &text[t.span.end..];
                (after.starts_with('(') && !after.starts_with("(s)"))
                    || after.strip_prefix("::").is_some_and(|rest| {
                        rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
                    })
            }
            _ => false,
        })
        .map(|t| t.span)
        .collect()
}

fn is_terminator(c: char) -> bool {
    matches!(c, '.' | '?' | '!' | '。' | '？' | '！')
}

/// Where each sentence of `text` starts, in order, beginning with 0.
///
/// The boundaries [`yp_lang::question`] reads: a line break, or a run of
/// terminal punctuation not followed by more of a name, so the dot in
/// "login.rs" ends nothing. Nothing inside code ends a sentence.
fn sentence_starts(text: &str, tokens: &[Token]) -> Vec<usize> {
    let code: Vec<Span> = tokens
        .iter()
        .filter(|t| t.kind == TokenKind::CodeSpan)
        .map(|t| t.span)
        .collect();
    let mut starts = vec![0];
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if covered_by(&code, i) {
            continue;
        }
        if c == '\n' {
            starts.push(i + 1);
            continue;
        }
        if !is_terminator(c) {
            continue;
        }
        let mut end = i + c.len_utf8();
        while let Some(&(j, d)) = chars.peek() {
            if !is_terminator(d) || covered_by(&code, j) {
                break;
            }
            end = j + d.len_utf8();
            chars.next();
        }
        let closes = text[end..]
            .chars()
            .next()
            .is_none_or(|n| !(n.is_ascii_alphanumeric() || matches!(n, '_' | '/' | '\\')));
        if closes {
            starts.push(end);
        }
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pronouns anchored in `text`, as text.
    fn anchored(text: &str) -> Vec<&str> {
        let tokens = yp_lang::tokenize(text);
        let hits = yp_lang::resources().unwrap().smells.find(text);
        anchor(text, &tokens, hits)
            .1
            .into_iter()
            .map(|s| s.slice(text))
            .collect()
    }

    #[test]
    fn a_pronoun_after_a_name_in_its_sentence_is_anchored() {
        assert_eq!(anchored("verify_token panics, fix it"), ["it"]);
        assert_eq!(
            anchored(
                "In src/auth/login.rs, change verify_token so it returns \
                 Err(AuthError::Expired) instead of panicking on an expired token."
            ),
            ["it"]
        );
    }

    #[test]
    fn every_shape_of_name_anchors() {
        for text in [
            "verify_token panics, fix it",
            "parseConfig panics, fix it",
            "AuthError is wrong, fix it",
            "os.path breaks, fix it",
            "Foo::bar breaks, fix it",
            "src/auth/login.rs breaks, fix it",
            "login() panics, fix it",
            "`verify` panics, fix it",
        ] {
            assert_eq!(anchored(text), ["it"], "{text}");
        }
    }

    #[test]
    fn a_pronoun_with_no_name_before_it_in_its_sentence_is_not() {
        for text in [
            "fix it",
            "it's broken again",
            "make it better",
            // Before the name, not after it.
            "fix it in verify_token",
            // A new sentence.
            "Fix verify_token. It panics on expired tokens.",
            "verify_token panics\nfix it",
            // Shaped like a name, but not one.
            "e.g. it should return 0",
            "update the file(s) so it compiles",
            "Config is wrong, fix it",
        ] {
            assert!(anchored(text).is_empty(), "{text}: {:?}", anchored(text));
        }
    }

    #[test]
    fn a_pasted_block_is_not_a_name() {
        let text = "```\nasdf_qwer zxcv_uiop\n``` fix it";
        assert!(anchored(text).is_empty());
    }

    #[test]
    fn pronouns_that_reach_back_into_the_conversation_are_not_anchored() {
        assert!(anchored("verify_token should do the same").is_empty());
        assert!(anchored("verify_token 아까 고쳤잖아").is_empty());
    }

    #[test]
    fn korean_follows_the_same_rule() {
        assert_eq!(anchored("verify_token 그거 왜 이래?"), ["그거"]);
        assert!(anchored("그거 다시 해줘").is_empty());
    }

    #[test]
    fn a_terminator_inside_code_or_a_name_ends_no_sentence() {
        assert_eq!(anchored("`a.b()?` panics. fix it"), Vec::<&str>::new());
        assert_eq!(anchored("`a.b()?` panics, fix it"), ["it"]);
        assert_eq!(anchored("login.rs panics, fix it"), ["it"]);
    }
}
