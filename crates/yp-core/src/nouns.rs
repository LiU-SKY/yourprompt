//! An action word used as a noun names the work, not more of it.
//!
//! The cue lexicon lists words, and most English action verbs are nouns as
//! often as verbs. Read as verbs, the nouns were extra objectives: "add a test
//! for verify_token" and "fix the failing test in tests/auth.rs" each asked
//! for one thing and kept 29.6 of the 40 singularity points, with "test"
//! marked as the action. Counting "run" as an action in v0.1.1 cost "add a run
//! script to package.json" 16 points (527.1 to 511.1), and "the test run is
//! flaky, fix the retry in tests/auth.rs" was read as four objectives.
//!
//! A verb cannot follow a determiner. So an action word directly after one --
//! "a", "the", "this", "every", "your", a possessive like "the parser's" -- is
//! a noun. So is one directly after a word that only ever modifies a noun
//! ("failing", "flaky", "unit", "dry"), and one directly after another action
//! word, whose object or compound it is: "run" in "the test run", "test" in
//! "don't change test files". And so is one quoted on its own, which is
//! mentioned rather than used: the "build" job, `Output: "test"`. Three of
//! HumanEvalComm's damaged versions of problem 143 expect the output "test".
//! Once the "a test" in its example input was a noun, that quoted word was
//! all the objective they had, and it put all three above the original. Such
//! a noun earns no objective, does not count as a second one, and is not
//! marked as the action.
//!
//! So a statement whose only action word is a noun asks for nothing. "the
//! build is broken" took "build" as its objective and scored 356.4, above
//! "fix the bug" at 305.4. It now scores 87.2, beside "it's broken again" at
//! 52.2 and its Korean counterpart, 빌드가 깨졌어, at 41.0.
//!
//! The lists leave out words that also open an instruction, so that a
//! mistake leaves a noun counted, as before, rather than drops a request.
//! "first", "next" and "then" order its steps, as in SWE-bench's own "first
//! delete unique_together, then do a makemigrations and then migrate".
//! "either" opens "either fix the parser or revert it", and "her" is out
//! because of "let her test it". "this" and "that" are in although they can
//! end a clause as pronouns. In SWE-bench Lite's 300 issues they stand before
//! an action word six times, and once, in "after that change the field", the
//! pronoun is followed by a real verb. Of the 82 action words taken for nouns
//! there, that is the only verb.
//!
//! "no" is in too. A prohibition, "fix the bug, no refactor", has been
//! settled into a scope constraint before this runs, so what follows "no"
//! here is a noun: "there is no fix yet", "with no retry".
//!
//! A line break ends the phrase, so a list item that begins with a verb keeps
//! it whatever the line above ends with.
//!
//! English only. A Korean verb carries its ending, 추가해 or 고쳐줘, which no
//! noun has. The loanwords the Korean lexicon also lists as actions
//! (리팩터링, 마이그레이션, 업데이트) are nouns that become verbs with 하다
//! after them, and telling those apart needs a morphological analyser.
//!
//! Grounding still reads such a noun as an instruction word rather than as a
//! name, as it does a forbidden verb, so the grounding axis does not move.

use yp_lang::{CueId, Hit};

/// Articles, demonstratives, quantifiers and possessive pronouns. Sorted, for
/// binary search.
const DETERMINERS: &[&str] = &[
    "a", "all", "an", "another", "any", "each", "every", "his", "its", "more", "my", "no", "one",
    "our", "some", "that", "the", "their", "these", "this", "those", "what", "which", "whose",
    "your",
];

/// Words that only ever stand before a noun: "the failing test", "a flaky
/// build", "a dry run", "its own log". Sorted, for binary search.
const MODIFIERS: &[&str] = &[
    "broken",
    "dry",
    "e2e",
    "existing",
    "extra",
    "failed",
    "failing",
    "flaky",
    "following",
    "integration",
    "missing",
    "new",
    "nightly",
    "other",
    "own",
    "passing",
    "same",
    "smoke",
    "unit",
];

/// `settled` without the English action words `text` uses as nouns.
///
/// `found` is every cue hit the matcher made, before any prohibition settled
/// one away: "test" in "don't change test files" is still the object of
/// "change". Hits other than action verbs pass through unchanged and in
/// order.
pub(crate) fn strip(text: &str, found: &[Hit<CueId>], settled: Vec<Hit<CueId>>) -> Vec<Hit<CueId>> {
    let mut actions: Vec<usize> = found
        .iter()
        .filter(|h| h.id == CueId::ActionVerb)
        .map(|h| h.span.end)
        .collect();
    actions.sort_unstable();
    settled
        .into_iter()
        .filter(|h| h.id != CueId::ActionVerb || !is_noun(text, h, &actions))
        .collect()
}

/// Whether the action word `hit` is a noun. `actions` holds where every action
/// word in the text ends, sorted.
fn is_noun(text: &str, hit: &Hit<CueId>, actions: &[usize]) -> bool {
    if !hit.term.is_ascii() {
        return false;
    }
    if quoted(text, hit) {
        return true;
    }
    let before = &text[..hit.span.start];
    let joined = before.trim_end_matches([' ', '\t']);
    // Nothing but spaces between the two words: not the start of the text, a
    // line break, or punctuation.
    if joined.len() == before.len() {
        return false;
    }
    if actions.binary_search(&joined.len()).is_ok() {
        return true;
    }
    let Some(word) = last_word(joined) else {
        return false;
    };
    let word = word.to_ascii_lowercase().replace('’', "'");
    DETERMINERS.binary_search(&word.as_str()).is_ok()
        || MODIFIERS.binary_search(&word.as_str()).is_ok()
        // "the parser's test", "it's test time"; "let's" is the one form of
        // "'s" that an instruction follows.
        || (word.ends_with("'s") && word != "let's")
}

/// Whether `hit` stands in quotes on its own, as in `Output: "test"` or the
/// "build" job: a word mentioned rather than used.
fn quoted(text: &str, hit: &Hit<CueId>) -> bool {
    let open = text[..hit.span.start].chars().next_back();
    let close = text[hit.span.end..].chars().next();
    matches!(
        (open, close),
        (Some('"'), Some('"'))
            | (Some('\''), Some('\''))
            | (Some('“'), Some('”'))
            | (Some('‘'), Some('’'))
    )
}

/// The word `s` ends with, if it ends with one.
fn last_word(s: &str) -> Option<&str> {
    let start = s
        .char_indices()
        .rev()
        .take_while(|&(_, c)| c.is_ascii_alphanumeric() || c == '\'' || c == '’')
        .last()
        .map(|(i, _)| i)?;
    Some(&s[start..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verbs(text: &str) -> Vec<String> {
        let found = yp_lang::resources().unwrap().cue_matcher.find(text);
        let settled = crate::negation::settle(text, &found);
        strip(text, &found, settled)
            .into_iter()
            .filter(|h| h.id == CueId::ActionVerb)
            .map(|h| h.term)
            .collect()
    }

    #[test]
    fn the_word_lists_are_sorted_so_the_search_works() {
        for list in [DETERMINERS, MODIFIERS] {
            let mut sorted = list.to_vec();
            sorted.sort_unstable();
            assert_eq!(list, sorted.as_slice());
        }
    }

    #[test]
    fn an_action_word_after_a_determiner_or_a_modifier_is_a_noun() {
        for (text, wanted) in [
            ("add a test for verify_token", vec!["add"]),
            ("fix the failing test in tests/auth.rs", vec!["fix"]),
            ("add a run script to package.json", vec!["add"]),
            (
                "write a test for verify_token in tests/auth.rs",
                vec!["write"],
            ),
            ("revert this change", vec!["revert"]),
            ("fix flaky test in the retry loop", vec!["fix"]),
            ("fix the parser's test", vec!["fix"]),
            ("the build is broken", vec![]),
            ("there is no fix yet", vec![]),
        ] {
            assert_eq!(verbs(text), wanted, "{text}");
        }
    }

    #[test]
    fn an_action_word_after_another_one_is_its_object() {
        assert_eq!(verbs("make the test run faster"), ["make"]);
        assert_eq!(
            verbs("the test run is flaky, fix the retry in tests/auth.rs"),
            ["fix"]
        );
        assert_eq!(verbs("run test suite on CI"), ["run"]);
        // Under a prohibition the object is still an object, not a request.
        assert!(verbs("don't change test files").is_empty());
        assert!(verbs("Don't add a test").is_empty());
    }

    #[test]
    fn a_verb_where_an_instruction_starts_stays_a_verb() {
        for (text, wanted) in [
            ("test the parser", vec!["test"]),
            ("run the tests", vec!["run"]),
            ("please test it", vec!["test"]),
            ("can you test this?", vec!["test"]),
            ("fix it and test it", vec!["fix", "test"]),
            ("fix the parser, then run the tests", vec!["fix", "run"]),
            (
                "first delete unique_together, then migrate",
                vec!["delete", "migrate"],
            ),
            ("let's fix the parser", vec!["fix"]),
            (
                "either fix the parser or revert the commit",
                vec!["fix", "revert"],
            ),
            ("you should log the error", vec!["log"]),
            // A line break ends the phrase.
            ("keep all of this\ntest the parser", vec!["test"]),
        ] {
            assert_eq!(verbs(text), wanted, "{text}");
        }
    }

    #[test]
    fn an_action_word_quoted_on_its_own_is_mentioned() {
        assert_eq!(verbs("add a \"test\" script to package.json"), ["add"]);
        assert_eq!(verbs("rename the 'build' job to “compile”"), ["rename"]);
        assert!(verbs("Input: sentence = \"lets go\"\nOutput: \"test\"").is_empty());
        // A quoted instruction is still an instruction.
        assert_eq!(verbs("the issue says \"fix the parser\""), ["fix"]);
    }

    #[test]
    fn a_prohibition_is_settled_before_the_noun_is_looked_for() {
        // "refactor" became the scope constraint; it is not dropped as a noun.
        let text = "fix the bug, no refactor";
        let found = yp_lang::resources().unwrap().cue_matcher.find(text);
        let settled = strip(text, &found, crate::negation::settle(text, &found));
        assert!(settled
            .iter()
            .any(|h| h.id == CueId::ScopeConstraint && h.term == "refactor"));
        assert_eq!(verbs(text), ["fix"]);
        // Not under a prohibition, what follows "no" is a noun.
        assert_eq!(verbs("add a test with no retry"), ["add"]);
    }

    #[test]
    fn korean_is_left_alone() {
        assert_eq!(verbs("테스트 추가해줘"), ["추가해"]);
        assert_eq!(
            verbs("마이그레이션 파일 추가해줘"),
            ["마이그레이션", "추가해"]
        );
    }

    #[test]
    fn hits_other_than_action_verbs_pass_through_in_order() {
        let text = "only in src/cli.rs, refactor parse_args so it returns Config; tests must pass";
        let found = yp_lang::resources().unwrap().cue_matcher.find(text);
        assert_eq!(strip(text, &found, found.clone()), found);
    }
}
