//! Questions, counted as requests.
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
//!
//! A yes/no question can also be asked by the verb that wants it answered,
//! with no question mark: "Check whether Token::decode can panic on any
//! input.", "Token::decode 가 어떤 입력에서 panic 할 수 있는지 확인해줘.".
//! "check" and 확인해 are not actions, because on their own they do not say
//! what to look for. The clause after "whether" or "if", or the one before
//! -지, does, so such a request counts as the question it asks, by the rules a
//! question with a question mark follows. Once "decode" was read as part of
//! the name, these two asked for nothing and scored 376.7 and 301.9; they
//! score 530.5 and 455.7, what they scored while "decode" was mistaken for
//! their action. Only an imperative asks, as only an opening "Return" does:
//! "Your job is to check if ..." in HumanEvalComm's problem 119 describes the
//! function, and read as a question it put the version cut short after that
//! sentence above the original, 628.4 to 620.4.

use yp_lang::question::Question;
use yp_lang::{CueId, Hit, SmellHit, SmellId, Span, Token, TokenKind};

use crate::prompt::is_pasted_block;

/// English verbs that ask the yes/no question of the clause after "whether"
/// or "if": "check whether the cache is stale". Sorted, for binary search.
const EN_ASKING: &[&str] = &["check", "determine"];

/// Korean words that ask the question of the clause before them, when it
/// ends in -지: 버그인지 확인해줘, 통과하는지 체크해줘. 확인하는 describes,
/// as in 반환하는지 확인하는 테스트.
const KO_ASKING: &[&str] = &["알아봐", "체크해", "확인 좀", "확인해"];

/// The questions in `text` that count as requests, direct ones and those a
/// verb asks, in text order, and `smell_hits` without the interrogatives that
/// asked them.
///
/// `smell_hits` must already be without the pronouns pasted material
/// anchors, and, with no index, those a name before them anchors, so any
/// deictic still among them points at nothing in the prompt. `material` says
/// whether the prompt carries pasted material that reads as language.
pub fn asked(
    text: &str,
    tokens: &[Token],
    smell_hits: Vec<SmellHit>,
    material: bool,
) -> (Vec<Hit<CueId>>, Vec<SmellHit>) {
    // In text order, one per sentence and inside it, so the question words
    // never overlap. Every lookup below is a binary search: scanning this
    // list for every smell and every token, and the smells for every
    // question, was quadratic, and a prompt that is nothing but "why? "
    // repeated is all questions.
    let found = yp_lang::question::find(text, tokens);
    let smell_hits: Vec<SmellHit> = smell_hits
        .into_iter()
        .filter(|s| {
            // Only the last question word starting at or before the smell
            // can contain it.
            let before = found.partition_point(|q| q.hit.span.start <= s.span.start);
            !found[..before]
                .last()
                .is_some_and(|q| q.interrogative && s.span.end <= q.hit.span.end)
        })
        .collect();

    let says_something = material
        || tokens.iter().any(|t| {
            t.kind != TokenKind::Punct
                && !yp_lang::is_filler(t)
                && !is_pasted(text, t)
                && !asks(&found, t)
        });
    if !says_something {
        return (Vec::new(), smell_hits);
    }

    let named = names_something(text, tokens);
    // Where the deictics still charged start, sorted so that a question
    // finds the first one at or after its sentence by binary search.
    let mut deictics: Vec<usize> = smell_hits
        .iter()
        .filter(|s| s.id == SmellId::VaguePronoun && yp_lang::is_deictic(&s.term))
        .map(|s| s.span.start)
        .collect();
    deictics.sort_unstable();
    let embedded = embedded(text, tokens, &found);
    let mut questions: Vec<Hit<CueId>> = found
        .into_iter()
        .chain(embedded)
        .filter(|q| {
            named || {
                let first = deictics.partition_point(|&d| d < q.sentence.start);
                !deictics.get(first).is_some_and(|&d| q.sentence.contains(d))
            }
        })
        .map(|q| q.hit)
        .collect();
    questions.sort_by_key(|q| q.span.start);
    (questions, smell_hits)
}

/// The yes/no questions in `text` that a verb asks rather than a question
/// mark, outside the sentences of the `found` direct ones: "can you check
/// whether it panics?" is one question, not two.
fn embedded(text: &str, tokens: &[Token], found: &[Question]) -> Vec<Question> {
    let ends = sentence_ends(text);
    let mut out = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        let asks = match token.kind {
            // Only an imperative asks, as only an opening "Return" does: "I'll
            // check if it works" and "Your job is to check if ..." say what
            // someone will do.
            TokenKind::Word => {
                EN_ASKING.binary_search(&token.text.as_str()).is_ok()
                    && tokens.get(i + 1).is_some_and(|next| {
                        next.kind == TokenKind::Word
                            && matches!(next.text.as_str(), "whether" | "if")
                    })
                    && crate::nouns::opens_clause(text, token.span.start)
            }
            TokenKind::Hangul => {
                KO_ASKING
                    .iter()
                    .any(|k| text[token.span.start..].starts_with(k))
                    && i > 0
                    && tokens[i - 1].kind == TokenKind::Hangul
                    && text[tokens[i - 1].span.end..token.span.start]
                        .trim()
                        .is_empty()
                    && asks_whether(&tokens[i - 1].text)
            }
            _ => false,
        };
        if !asks {
            continue;
        }
        let at = token.span.start;
        // The sentence it stands in, between the ends around it.
        let after = ends.partition_point(|&e| e <= at);
        let sentence = Span::new(
            after.checked_sub(1).map_or(0, |b| ends[b]),
            ends.get(after).copied().unwrap_or(text.len()),
        );
        let direct = found.partition_point(|q| q.sentence.end <= at);
        if found.get(direct).is_some_and(|q| q.sentence.contains(at)) {
            continue;
        }
        out.push(Question {
            hit: Hit {
                id: CueId::Question,
                term: token.span.slice(text).to_string(),
                span: token.span,
            },
            sentence,
            interrogative: false,
        });
    }
    out
}

/// Whether the Korean word `word` ends a clause in -지 that a verb after it
/// asks about: 있는지, 버그인지, 될지, and with a particle, 있는지를. The
/// syllable before 지 closes on ㄴ or ㄹ, which 페이지, 메시지 and 까지 do not.
fn asks_whether(word: &str) -> bool {
    let word = word.strip_suffix(['를', '도']).unwrap_or(word);
    let mut syllables = word.chars().rev();
    syllables.next() == Some('지')
        && syllables.next().is_some_and(|c| {
            // A precomposed syllable's final consonant is its offset from
            // U+AC00 modulo 28; ㄴ is 4 and ㄹ is 8.
            matches!(c as u32, 0xAC00..=0xD7A3) && matches!((c as u32 - 0xAC00) % 28, 4 | 8)
        })
}

/// Where the sentences of `text` end: at a line break, and at sentence
/// punctuation followed by a space or by the end. Sorted.
fn sentence_ends(text: &str) -> Vec<usize> {
    let mut ends = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let closes = c == '\n'
            || (matches!(c, '.' | '!' | '?' | '。' | '？' | '！')
                && chars.peek().is_none_or(|&(_, n)| n.is_whitespace()));
        if closes {
            ends.push(i + c.len_utf8());
        }
    }
    ends
}

/// Whether `token` holds the word that asks one of the `found` questions.
fn asks(found: &[Question], token: &Token) -> bool {
    let first = found.partition_point(|q| q.hit.span.start < token.span.start);
    found[first..]
        .iter()
        .take_while(|q| q.hit.span.start < token.span.end)
        .any(|q| q.hit.span.end <= token.span.end)
}

/// A pasted block, or the marker left where one was cut out.
fn is_pasted(text: &str, token: &Token) -> bool {
    token.kind == TokenKind::CodeSpan && {
        let raw = token.span.slice(text);
        is_pasted_block(raw) || raw == crate::params::prompt::ATTACHMENT_MARKER
    }
}

/// Whether the typed instruction names anything a pronoun could stand for, by
/// the test of a name this shares with the antecedents and the noun reading,
/// [`crate::antecedents::is_name`]: "Why does login() panic when it gets an
/// expired token?" asks about login as plainly as the same question asks
/// about verify_token.
///
/// A pasted block is not a name. It anchors pronouns by its own rule, which
/// reads what is in it; counting it here would let a fence around anything at
/// all rescue a question.
fn names_something(text: &str, tokens: &[Token]) -> bool {
    (0..tokens.len()).any(|i| crate::antecedents::is_name(text, tokens, i))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

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
    fn a_call_or_a_qualified_name_is_something_to_ask_about() {
        // "it" was anchored to login() while the question was dropped as
        // being about nothing: 480.4 against 634.2 for verify_token.
        for text in [
            "Why does login() panic when it gets an expired token?",
            "Why does Session::refresh panic when it gets an expired token?",
        ] {
            assert_eq!(questions(text), ["Why"], "{text}");
            let corpus = crate::MapCorpus::new(10, &[("login", 1, 2, 1), ("refresh", 1, 2, 1)]);
            for corpus in [None, Some(&corpus as &dyn crate::Corpus)] {
                let s = crate::score_with(text, corpus).unwrap();
                let objective = &s.actionability.components[0];
                assert_eq!(
                    objective.earned,
                    crate::params::actionability::OBJECTIVE_MAX,
                    "{text}: {}",
                    objective.detail
                );
            }
        }
    }

    #[test]
    fn a_verb_that_asks_whether_asks_a_question() {
        for (text, wanted) in [
            (
                "Check whether Token::decode can panic on any input.",
                "Check",
            ),
            (
                "Please check if verify_token handles an empty token.",
                "check",
            ),
            (
                "Determine whether parse_args accepts an empty list.",
                "Determine",
            ),
            (
                "Token::decode 가 어떤 입력에서 panic 할 수 있는지 확인해줘.",
                "확인해줘",
            ),
            ("verify_token 이 빈 토큰을 받는지 체크해줘", "체크해줘"),
            ("parse_args 가 빈 리스트를 받을지 알아봐줘", "알아봐줘"),
        ] {
            assert_eq!(questions(text), [wanted], "{text}");
        }
        // Said of what someone will do, or of something other than a
        // clause, it asks nothing; and with a question mark it is one
        // question, not two.
        for text in [
            "I'll check if verify_token works and get back to you.",
            "Your job is to check if it is possible to parse the list.",
            "Check the logs in src/server.rs.",
            "check, if needed, the logs in src/server.rs",
            "로그인 페이지 확인해줘.",
            "에러 메시지 확인해줘.",
            "여기까지 확인해줘.",
            "반환하는지 확인하는 테스트",
        ] {
            assert!(questions(text).is_empty(), "{text}: {:?}", questions(text));
        }
        assert_eq!(
            questions("Can you check whether verify_token panics?"),
            ["Can"]
        );
        // About a pronoun that points at nothing, it asks about nothing.
        assert!(questions("Check whether it works.").is_empty());
    }

    #[test]
    fn a_question_a_verb_asks_is_the_objective_in_both_languages() {
        // Both earned no objective once "decode" was read as part of the
        // name: 376.7 and 301.9.
        for text in [
            "Check whether Token::decode can panic on any input.",
            "Token::decode 가 어떤 입력에서 panic 할 수 있는지 확인해줘.",
        ] {
            let s = crate::score(text).unwrap();
            let objective = &s.actionability.components[0];
            assert_eq!(
                objective.earned,
                crate::params::actionability::OBJECTIVE_MAX,
                "{text}: {}",
                objective.detail
            );
        }
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

    #[test]
    fn many_questions_cost_about_what_as_many_plain_ones_do() {
        // Every token, smell and question used to be checked against every
        // question in the prompt. Each pair below finds the same questions,
        // but in the first text of a pair those checks are the work: it says
        // nothing besides its questions, puts a deictic in every one, or asks
        // them all in one sentence that a verb's question looks up. At
        // 80,000 and 20,000 questions, the checks took 7.9 s against 68 ms
        // and 21.6 s against 26 ms in a debug build. Timed against each
        // other, the two texts measure how the work grows, not how fast the
        // machine is.
        let time = |text: &str| {
            let tokens = yp_lang::tokenize(text);
            let smells = yp_lang::resources().unwrap().smells.find(text);
            (0..3)
                .map(|_| {
                    let smells = smells.clone();
                    let start = Instant::now();
                    std::hint::black_box(asked(text, &tokens, smells, false));
                    start.elapsed()
                })
                .min()
                .unwrap()
        };
        for (heavy, light) in [
            (
                "why? ".repeat(80_000),
                format!("the build fails. {}", "why? ".repeat(80_000)),
            ),
            (
                "why is it broken? ".repeat(20_000),
                "why is the build broken? ".repeat(20_000),
            ),
            (
                "and check whether it works ".repeat(20_000),
                "and check whether the build works. ".repeat(20_000),
            ),
        ] {
            let (heavy_time, light_time) = (time(&heavy), time(&light));
            assert!(
                heavy_time < light_time * 20 + Duration::from_millis(100),
                "{:?}...: {heavy_time:?} against {light_time:?}",
                &heavy[..18]
            );
        }
    }
}
