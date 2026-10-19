//! What a prompt is, decided before anything is said about how well it is
//! written.
//!
//! Every prompt used to be graded, and a working session is full of prompts
//! that are not requests. "continue" scored 0.0, "yes" 15.1 and "ok do it"
//! 24.5, and each left an F in the status line in place of the task it was
//! continuing. `/score` reached the hook before Claude Code expanded it, was
//! graded 15.1, and was then what `/score` explained. A request that scores
//! 633.9 in English scored 149.1 in Spanish, because only English and Korean
//! can be read.
//!
//! So a prompt is classified first, and only a [`Kind::Task`] is graded. The
//! number is still computed for every kind; what to show is the caller's
//! decision.

use serde::{Deserialize, Serialize};
use yp_lang::Token;

/// What a prompt is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A request, graded as one. Anything that is not clearly one of the
    /// others is a task: grading a prompt that is not one only costs an F,
    /// while skipping a real one hides it.
    #[default]
    Task,
    /// A slash command such as `/score` or `/compact keep the API notes`.
    /// Claude Code runs it or expands it, so what was typed is not what the
    /// model reads.
    Command,
    /// A reply that only makes sense inside a running conversation: "yes",
    /// "continue", "다시 해줘".
    FollowUp,
    /// Prose in a language the lexicons do not read.
    Unsupported,
}

impl Kind {
    /// True for the one kind the score describes.
    pub fn is_graded(self) -> bool {
        self == Kind::Task
    }
}

/// A slash command: the first word is `/` and a name, with no second `/`
/// that would make it a path. `/review:security` is a command;
/// `/usr/local/bin is not on PATH` is not.
fn is_command(text: &str) -> bool {
    let Some(name) = text
        .split_whitespace()
        .next()
        .and_then(|word| word.strip_prefix('/'))
    else {
        return false;
    };
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'-'))
}

/// Classify a prompt, naming its language when it is [`Kind::Unsupported`]
/// and the evidence points at one.
///
/// Code is read the same way scoring reads it, so classifying the raw text
/// and classifying the instruction left after attachments are cut out agree:
/// both see code where the paste was.
pub fn kind_of(text: &str) -> (Kind, Option<&'static str>) {
    classify(text, &yp_lang::tokenize(text))
}

/// [`kind_of`] for a caller that has already tokenized the text.
pub(crate) fn classify(text: &str, tokens: &[Token]) -> (Kind, Option<&'static str>) {
    if is_command(text) {
        return (Kind::Command, None);
    }
    if yp_lang::resources().is_some_and(|r| r.follow_ups.is_follow_up(tokens)) {
        return (Kind::FollowUp, None);
    }
    if let Some(foreign) = yp_lang::foreign::detect(text) {
        return (Kind::Unsupported, foreign.name);
    }
    (Kind::Task, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(text: &str) -> Kind {
        kind_of(text).0
    }

    #[test]
    fn a_slash_command_is_a_command() {
        for text in [
            "/score",
            "/compact keep the API notes",
            "/review:security",
            "/yourprompt:score",
            "/security-review",
            "/loop 5m check the deploy",
            "/mcp__github__list_prs",
            "  /init",
        ] {
            assert_eq!(kind(text), Kind::Command, "{text:?}");
        }
    }

    #[test]
    fn a_path_or_a_slash_elsewhere_is_not_a_command() {
        for text in [
            "/usr/local/bin is not on PATH",
            "/api/login returns 500",
            "/",
            "/ score",
            "//comment out the call",
            "fix the /score command",
            "/점수",
        ] {
            assert_ne!(kind(text), Kind::Command, "{text:?}");
        }
    }

    #[test]
    fn english_follow_ups_are_recognised() {
        for text in [
            "yes",
            "y",
            "yeah",
            "yep",
            "sure",
            "ok",
            "okay",
            "go ahead",
            "continue",
            "proceed",
            "do it",
            "keep going",
            "next",
            "again",
            "try again",
            "retry",
            "thanks",
            "lgtm",
            "sounds good",
            "no",
            "stop",
            "wait",
            "cancel",
            "1",
            "2",
            "3",
            "option 2",
            "ok do it",
            "Yes, please.",
            "LGTM!",
            "ok, go ahead and continue 👍",
            "go",
            "do it again",
            "thank you!",
            "go with option 2",
        ] {
            assert_eq!(kind(text), Kind::FollowUp, "{text:?}");
        }
    }

    #[test]
    fn korean_follow_ups_are_recognised() {
        for text in [
            "응",
            "네",
            "넵",
            "ㅇㅇ",
            "ㅇㅋ",
            "오케이",
            "그래",
            "좋아",
            "계속",
            "계속해",
            "계속해줘",
            "진행해",
            "진행해줘",
            "해줘",
            "다시",
            "다시 해줘",
            "고마워",
            "감사합니다",
            "아니",
            "멈춰",
            "그만",
            "1번",
            "둘 다",
            "응 계속 진행해주세요",
            "네, 2번으로 해줘",
            "좋아 진행시켜",
            "그만둬",
        ] {
            assert_eq!(kind(text), Kind::FollowUp, "{text:?}");
        }
    }

    #[test]
    fn a_task_is_never_taken_for_a_follow_up() {
        // Anything that names something, carries an action verb, or points
        // at something with a pronoun is a request, and is graded.
        for text in [
            "그거 다시 해줘",
            "fix it",
            "it's broken again",
            "make it better",
            "fix the bug",
            "do that again with tests",
            "run the tests",
            "looks good, commit it",
            "undo that",
            "no, use postgres instead",
            "yes, and add a test for it",
            "continue the refactor in src/cli.rs",
            "retry the upload on a 429",
            "stop using unwrap in parse_args",
            "wait, why did you delete main.rs?",
            "다시 테스트 돌려줘",
            "계속 진행하되 테스트는 건드리지 마",
            "1번 방식으로 login 고쳐줘",
            "통과시켜줘",
            "그대로 둬",
            "응 그렇게 해줘",
            "ok `continue`",
            "ok #123",
            "2024",
            // One word of work is still work, however short.
            "test",
            "commit",
            "deploy",
            "undo",
            "커밋해",
            "배포해",
        ] {
            assert_eq!(kind(text), Kind::Task, "{text:?}");
        }
    }

    #[test]
    fn saying_nothing_is_not_a_reply() {
        // Fillers, punctuation and emoji are skipped inside a reply, but on
        // their own they are graded as before -- and score near the floor.
        for text in ["", "   ", "hmm", "!!!", "👍", "음... 어..."] {
            assert_eq!(kind(text), Kind::Task, "{text:?}");
        }
    }

    #[test]
    fn a_reply_carrying_an_attachment_is_a_task() {
        let file = "fn main() {}\n".repeat(40);
        assert_eq!(kind(&format!("continue\n```rust\n{file}```")), Kind::Task);
    }

    #[test]
    fn a_foreign_prompt_is_unsupported_and_named() {
        assert_eq!(
            kind_of(
                "arregla el bug en src/auth/login.rs para que verify_token devuelva \
                 Err(AuthError::Expired). Los tests deben pasar."
            ),
            (Kind::Unsupported, Some("Spanish"))
        );
        assert_eq!(
            kind_of(
                "src/auth/login.rs の verify_token を修正して、期限切れトークンで panic \
                 せずに Err(AuthError::Expired) を返すようにしてください。tests/auth.rs は通ること。"
            ),
            (Kind::Unsupported, Some("Japanese"))
        );
    }

    #[test]
    fn a_script_is_evidence_on_its_own_but_a_few_latin_words_are_not() {
        // Kana cannot be English or Korean however short the reply, so "はい"
        // is named. A Latin-script language needs a sentence's worth of
        // function words, so a short phrase stays a task and is graded.
        assert_eq!(kind_of("はい"), (Kind::Unsupported, Some("Japanese")));
        assert_eq!(kind_of("продолжай"), (Kind::Unsupported, Some("Russian")));
        for text in ["sí, continúa", "oui", "ja bitte", "arregla el bug"] {
            assert_eq!(kind(text), Kind::Task, "{text:?}");
        }
    }

    #[test]
    fn only_an_unsupported_prompt_has_a_language() {
        for text in ["fix the bug", "/score", "yes", "그거 고쳐줘", "continue"] {
            assert_eq!(kind_of(text).1, None, "{text:?}");
        }
    }

    #[test]
    fn kinds_serialize_in_snake_case() {
        let names: Vec<String> = [Kind::Task, Kind::Command, Kind::FollowUp, Kind::Unsupported]
            .iter()
            .map(|k| serde_json::to_string(k).unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "\"task\"",
                "\"command\"",
                "\"follow_up\"",
                "\"unsupported\""
            ]
        );
    }
}
