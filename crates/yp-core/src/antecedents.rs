//! A pronoun after a name stands for that name.
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
//! mean a name that resolves. What is shaped like code is decided by
//! [`is_name`]. Such a pronoun is not a smell, and it is marked as the
//! grounded one is. Whether the name exists is what an index would say, so a
//! made-up name anchors a pronoun as well as a real one, just as it scores as
//! well as one everywhere else without an index.
//!
//! A pronoun that opens a sentence also reaches back into the sentence before
//! it. That is how a bug report goes on about what it just named:
//! "verify_token returns None. It should return Err(AuthError::Expired)."
//! scored 348.8 with the "It" charged, and 509.1 with a semicolon for the full
//! stop, while grounding, which looks back over the whole prompt, anchored
//! it either way. "Fix verify_token. It panics on expired tokens." reads the
//! same, so its "It" is anchored too. The reach is still short of grounding's,
//! because here nothing confirms that the name is one: one sentence back, and
//! only for the pronoun the new sentence opens with, which is the one that
//! carries its subject on. "verify_token panics" on one line and "fix it" on
//! the next stays charged, and so does a pronoun two sentences on. "fix it",
//! "it's broken again" and "make it better" name nothing and stay charged. A
//! pasted block is not a name: it anchors pronouns by its own rule, which
//! reads what is in it, and counting it here would let a fence around
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

/// Words that may come before the pronoun a sentence opens with, without
/// taking its place: "But it panics", "Now it hangs", "그리고 그거 고쳐줘".
/// Sorted, for binary search.
const LEAD_INS: &[&str] = &[
    "also",
    "and",
    "but",
    "currently",
    "however",
    "instead",
    "now",
    "otherwise",
    "so",
    "still",
    "then",
    "yet",
    "그래서",
    "그런데",
    "그리고",
    "근데",
];

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
    let openers = openers(tokens, &starts);
    // Whether a name lies wholly within `from..to`.
    let named = |from: usize, to: usize| {
        let first = names.partition_point(|n| n.start < from);
        names.get(first).is_some_and(|n| n.end <= to)
    };
    let (anchored, kept): (Vec<SmellHit>, Vec<SmellHit>) = hits.into_iter().partition(|h| {
        h.id == SmellId::VaguePronoun && yp_lang::is_deictic(&h.term) && {
            let k = starts.partition_point(|&s| s <= h.span.start) - 1;
            named(starts[k], h.span.start)
                || (k > 0 && openers[k] == Some(h.span.start) && named(starts[k - 1], starts[k]))
        }
    });
    (kept, anchored.into_iter().map(|h| h.span).collect())
}

/// Where the word each sentence opens with starts, past punctuation, fillers
/// and lead-ins, one entry per start in `starts`.
///
/// Read once for the whole text. Asked of every pronoun from the start of its
/// sentence, it was quadratic in a sentence of lead-ins followed by pronouns.
fn openers(tokens: &[Token], starts: &[usize]) -> Vec<Option<usize>> {
    let mut out = vec![None; starts.len()];
    for t in tokens {
        let k = starts.partition_point(|&s| s <= t.span.start) - 1;
        let lead = t.kind == TokenKind::Punct
            || yp_lang::is_filler(t)
            || LEAD_INS
                .binary_search(&t.text.to_lowercase().as_str())
                .is_ok();
        if out[k].is_none() && !lead {
            out[k] = Some(t.span.start);
        }
    }
    out
}

/// Whether `tokens[i]` is shaped like a name in code: an identifier
/// (snake_case, CamelCase, dotted like `mod.attr`), a path, inline code, a
/// call (`login()`), or a part of a qualified name (`Foo::bar`).
///
/// The one test the questions, this module and the noun reading share. They
/// used to answer it apart: questions knew identifiers, paths and inline code,
/// this module also knew calls and qualified names, and the noun reading knew
/// none of them. So "Why does login() panic when it gets an expired token?"
/// had its "it" anchored here and its question dropped as being about nothing
/// (480.4, against 634.2 for verify_token), and "build() is broken, fix it"
/// counted build as a second objective. A word inside a name is part of the
/// name: "test" in `src/test/foo.rs` and "handler" in `src/handler.rs` are
/// neither an action nor a vague noun.
///
/// A plain word is a name when it is called, `login()` but not the plural
/// "file(s)", and when it stands on either side of `::`. A pasted block is not
/// a name; it anchors pronouns by its own rule. "e.g." is dotted like
/// `mod.attr` and is not one either.
pub(crate) fn is_name(text: &str, tokens: &[Token], i: usize) -> bool {
    let t = &tokens[i];
    match t.kind {
        TokenKind::Ident => !ABBREVIATIONS.contains(&t.text.to_ascii_lowercase().as_str()),
        TokenKind::Path => true,
        TokenKind::CodeSpan => {
            let raw = t.span.slice(text);
            !is_pasted_block(raw) && raw != ATTACHMENT_MARKER
        }
        TokenKind::Word => {
            let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
            let after = &text[t.span.end..];
            let before = &text[..t.span.start];
            (after.starts_with('(') && !after.starts_with("(s)"))
                || after
                    .strip_prefix("::")
                    .is_some_and(|rest| rest.starts_with(ident))
                || before
                    .strip_suffix("::")
                    .is_some_and(|rest| rest.ends_with(ident))
        }
        _ => false,
    }
}

/// The spans of the tokens shaped like code, in order.
pub(crate) fn names(text: &str, tokens: &[Token]) -> Vec<Span> {
    (0..tokens.len())
        .filter(|&i| is_name(text, tokens, i))
        .map(|i| tokens[i].span)
        .collect()
}

/// Whether `span` lies within one of the sorted, disjoint `names`.
pub(crate) fn inside(names: &[Span], span: Span) -> bool {
    let at = names.partition_point(|n| n.start <= span.start);
    at > 0 && names[at - 1].end >= span.end
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
            // A new sentence that does not open with the pronoun.
            "verify_token panics\nfix it",
            "verify_token panics. Fix it.",
            // Two sentences back.
            "verify_token panics. The parser is fine. It runs first.",
            // Shaped like a name, but not one.
            "e.g. it should return 0",
            "update the file(s) so it compiles",
            "Config is wrong, fix it",
        ] {
            assert!(anchored(text).is_empty(), "{text}: {:?}", anchored(text));
        }
    }

    #[test]
    fn a_pronoun_that_opens_a_sentence_reaches_back_one() {
        // Grounding anchored these "It"s and this rule charged them: 348.8
        // without an index, 509.1 with a semicolon for the full stop.
        for (text, pronoun) in [
            (
                "verify_token returns None. It should return Err(AuthError::Expired).",
                "It",
            ),
            ("Fix verify_token. It panics on expired tokens.", "It"),
            ("login() hangs. But now it also leaks.", "it"),
            ("- `parse` returns None\n- it should return Err", "it"),
            ("verify_token 이 panic 나. 그거 고쳐줘.", "그거"),
        ] {
            assert_eq!(anchored(text), [pronoun], "{text}");
        }
    }

    #[test]
    fn both_modes_read_a_full_stop_the_same_way() {
        // Grounded, the "It" was anchored; ungrounded, it was charged.
        let text = "verify_token returns None. It should return Err(AuthError::Expired).";
        let s = crate::score(text).unwrap();
        assert!(
            !s.clarity.components[0].detail.contains("vague pronoun"),
            "{}",
            s.clarity.components[0].detail
        );
        let it = s
            .marks
            .iter()
            .find(|m| &text[m.start..m.end] == "It")
            .unwrap();
        assert_eq!(it.kind, "pronoun:anchored");
        let corpus = crate::MapCorpus::new(10, &[("verify_token", 1, 3, 1)]);
        let grounded = crate::score_with(text, Some(&corpus)).unwrap();
        let it = grounded
            .marks
            .iter()
            .find(|m| &text[m.start..m.end] == "It")
            .unwrap();
        assert_eq!(it.kind, "pronoun:anchored");
    }

    #[test]
    fn a_name_is_a_name_in_every_pass_s_eyes() {
        let named = |text: &str| -> Vec<String> {
            let tokens = yp_lang::tokenize(text);
            names(text, &tokens)
                .into_iter()
                .map(|s| s.slice(text).to_string())
                .collect()
        };
        assert_eq!(
            named("Why does login() panic when it gets an expired token?"),
            ["login"]
        );
        assert_eq!(
            named("Session::refresh and Self::new()"),
            ["Session", "refresh", "Self", "new"]
        );
        assert_eq!(
            named("fix `parse` in src/cli.rs and os.path"),
            ["`parse`", "src/cli.rs", "os.path"]
        );
        assert!(named("fix the test(s), e.g. the parser").is_empty());
        assert!(named("```\nasdf_qwer zxcv_uiop\n```").is_empty());
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
    fn many_pronouns_after_lead_ins_cost_about_what_as_many_words_do() {
        // Each pronoun once read its sentence again from the start to see
        // whether it opened it, so a sentence of lead-ins followed by
        // pronouns was quadratic. Timed against each other, the two texts
        // measure how the work grows, not how fast the machine is.
        use std::time::{Duration, Instant};
        let time = |text: &str| {
            let tokens = yp_lang::tokenize(text);
            let hits = yp_lang::resources().unwrap().smells.find(text);
            (0..3)
                .map(|_| {
                    let hits = hits.clone();
                    let start = Instant::now();
                    std::hint::black_box(anchor(text, &tokens, hits));
                    start.elapsed()
                })
                .min()
                .unwrap()
        };
        let lead = "and ".repeat(8_000);
        let heavy = time(&format!(
            "verify_token panics. {lead}{}",
            "it ".repeat(10_000)
        ));
        let light = time(&format!(
            "verify_token panics. {lead}{}",
            "ok ".repeat(10_000)
        ));
        assert!(
            heavy < light * 20 + Duration::from_millis(100),
            "pronouns {heavy:?}, words {light:?}"
        );
    }

    #[test]
    fn a_terminator_inside_code_or_a_name_ends_no_sentence() {
        assert_eq!(anchored("`a.b()?` panics. fix it"), Vec::<&str>::new());
        assert_eq!(anchored("`a.b()?` panics, fix it"), ["it"]);
        assert_eq!(anchored("login.rs panics, fix it"), ["it"]);
    }
}
