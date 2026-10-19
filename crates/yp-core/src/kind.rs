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

/// Directories found at the root of a filesystem. A prompt that opens with
/// one is about that directory -- "/tmp is full, clean it up" -- and was
/// skipped as a command, leaving the previous task's grade in the status
/// line. `dev` and `run` are left off: they are as likely to be someone's
/// own command. Sorted, for binary search.
const ROOT_DIRS: &[&str] = &[
    "Applications",
    "Library",
    "System",
    "Users",
    "Volumes",
    "bin",
    "boot",
    "etc",
    "home",
    "lib",
    "lib64",
    "media",
    "mnt",
    "nix",
    "opt",
    "private",
    "proc",
    "root",
    "sbin",
    "snap",
    "srv",
    "sys",
    "tmp",
    "usr",
    "var",
    "workspaces",
];

/// Verbs that, right after `/name`, make the prompt a sentence about that
/// route or path rather than a command's arguments: "/health returns 500
/// since the deploy". Sorted, for binary search.
const PATH_VERBS: &[&str] = &[
    "breaks",
    "broke",
    "crashed",
    "crashes",
    "errors",
    "failed",
    "fails",
    "gave",
    "gives",
    "hangs",
    "hung",
    "keeps",
    "leaks",
    "loads",
    "redirected",
    "redirects",
    "renders",
    "responded",
    "responds",
    "returned",
    "returns",
    "shows",
    "stopped",
    "stops",
    "threw",
    "throws",
    "works",
];

/// Auxiliaries that do the same: "/login should redirect to /dashboard".
/// They also open questions, and a question is what a command can take --
/// "/review is this safe to merge?" -- so they count only when the sentence
/// they open is not one. Sorted, for binary search.
const PATH_AUXILIARIES: &[&str] = &[
    "are",
    "aren't",
    "can't",
    "cannot",
    "did",
    "didn't",
    "does",
    "doesn't",
    "has",
    "hasn't",
    "is",
    "isn't",
    "should",
    "shouldn't",
    "was",
    "wasn't",
    "were",
    "weren't",
    "won't",
];

/// Claude Code's side question. It takes a question however it is
/// punctuated, so "/btw is this thread-safe" is a command, though "is"
/// makes "/health is down" a sentence about a route.
const SIDE_QUESTION: &str = "btw";

/// HTTP status codes in use. "/health 503 since the deploy" is about a
/// route; "/fix-issue 123" passes an issue number, so a status code only
/// counts with more of a sentence after it, and an unassigned code never
/// does.
fn is_status_code(word: &str) -> bool {
    let Ok(code) = word.parse::<u16>() else {
        return false;
    };
    word.len() == 3
        && matches!(code,
            100..=103 | 200..=208 | 226 | 300..=308 | 400..=418 | 421..=426 | 428 | 429
            | 431 | 451 | 500..=508 | 510 | 511)
}

/// A slash command: the first word is `/` and a name, with no second `/`
/// that would make it a path. `/review:security` is a command;
/// `/usr/local/bin is not on PATH` is not, and neither is a prompt about a
/// root directory or a route: "/tmp is full", "/health returns 500".
fn is_command(text: &str) -> bool {
    let mut words = text.split_whitespace();
    let Some(name) = words.next().and_then(|word| word.strip_prefix('/')) else {
        return false;
    };
    // A command's name starts with a letter: "/-- docs -/" opens a Lean
    // comment, which Claude Code sends to the model.
    let named = name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'-'));
    if !named || ROOT_DIRS.binary_search(&name).is_ok() {
        return false;
    }
    let Some(next) = words.next() else {
        return true;
    };
    if name.eq_ignore_ascii_case(SIDE_QUESTION) {
        return true;
    }
    let next = next
        .trim_end_matches(|c: char| c.is_ascii_punctuation() && c != '\'')
        .replace('\u{2019}', "'")
        .to_ascii_lowercase();
    if PATH_VERBS.binary_search(&next.as_str()).is_ok() {
        return false;
    }
    if PATH_AUXILIARIES.binary_search(&next.as_str()).is_ok() {
        return asks(text);
    }
    !(is_status_code(&next) && words.next().is_some())
}

/// True when the text's first sentence ends in a question mark.
fn asks(text: &str) -> bool {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if matches!(c, '.' | '!' | '?') && chars.peek().is_none_or(|n| n.is_whitespace()) {
            return c == '?';
        }
    }
    false
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
            "/commit fix the login test",
            "/code-review high",
            // An issue number, not a status code.
            "/fix-issue 123",
            "/fix-issue 404",
            // A question is what some commands take, and the side question
            // takes nothing else.
            "/review is this safe to merge?",
            "/btw is the cache per user?",
            "/btw does this leak on retry?",
            "/btw is this thread-safe",
            // Left off the root directories: on their own these are as
            // likely to be someone's own commands.
            "/dev",
            "/run",
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
            // Lean doc comments, which Claude Code sends to the model.
            "/-- The main theorem about lists -/ fails to compile",
            "/-! module docs -/ break the build",
        ] {
            assert_ne!(kind(text), Kind::Command, "{text:?}");
        }
    }

    #[test]
    fn a_sentence_about_a_root_directory_or_a_route_is_a_task() {
        // Each of these was skipped as a command, and the status line kept
        // showing the previous task's grade.
        for text in [
            "/tmp is full",
            "/tmp is full, clean it up",
            "/var is out of space",
            "/etc is read-only in the container",
            "/opt has an old node, remove it",
            "/Users is not writable on the runner",
            "/nix store is huge, gc it",
            "/tmp",
            "/dev is missing in the chroot",
            "/health returns 500 since the deploy; fix the handler in src/routes.rs \
             so it returns 200",
            "/login should redirect to /dashboard after a successful sign-in",
            "/checkout throws a TypeError on an empty cart",
            "/users isn't paginated",
            "/health 503 since the deploy",
        ] {
            assert_eq!(kind(text), Kind::Task, "{text:?}");
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
            "correct",
            "yes, that's correct",
            "done",
            "sgtm",
            "+1",
            "b",
            "(b)",
            "option B",
            // Claude Code's own resume prompt.
            "Continue from where you left off.",
            "continue where you left off",
            "Proceed with the plan.",
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
            "넵 ㅎㅎ",
            "응ㅋㅋㅋ",
            "계속해줘요",
            "진행 부탁드립니다",
            "부탁해",
            "넘어가",
            "다음으로",
            "확인했어요",
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
            // A request to check something, however it is spaced: "확인" is
            // not a reply because "확인" and "해줘" would make this one.
            "확인해줘",
            "확인해 줘",
            "다시 확인해줘",
            "체크해줘",
            "봐줘",
            "확인 부탁해",
            // A report, asked, asks for one.
            "done?",
            "확인했어?",
            "correct the typo in the README",
            "option e",
            "proceed with the migration",
            "continue where you left off in src/cli.rs",
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
        // is named. A Latin-script language needs three words or more, and
        // in a phrase that short more evidence than "arregla el bug" holds,
        // so these stay tasks and are graded.
        assert_eq!(kind_of("はい"), (Kind::Unsupported, Some("Japanese")));
        assert_eq!(kind_of("продолжай"), (Kind::Unsupported, Some("Russian")));
        for text in ["sí, continúa", "oui", "ja bitte", "arregla el bug"] {
            assert_eq!(kind(text), Kind::Task, "{text:?}");
        }
    }

    #[test]
    fn a_short_request_in_another_language_is_unsupported() {
        // Each of these was graded, an F.
        for (text, language) in [
            ("corrige l'erreur de connexion", "French"),
            ("arregla el error del login", "Spanish"),
            ("behebe den Login-Fehler", "German"),
            ("napraw błąd w logowaniu", "Polish"),
            (
                "fixa buggen i src/auth/login.rs så att verify_token returnerar ett fel \
                 när token har gått ut",
                "Swedish",
            ),
        ] {
            assert_eq!(
                kind_of(text),
                (Kind::Unsupported, Some(language)),
                "{text:?}"
            );
        }
    }

    #[test]
    fn english_that_quotes_or_names_another_language_is_a_task() {
        // Each of these was reported as the language it quotes, and the list
        // of names as German.
        for text in [
            "ru.json has 'Сохранить' twice",
            "Commit message 'Korrigiere den Fehler beim Login und aktualisiere die Tests' \
             is in German, rewrite in English",
            "Translate 'Bonjour le monde, comment ça va' to English",
            "Missing translation: 'Вы уверены, что хотите удалить этот файл?'",
            "Seed users: Ursula von der Leyen, Mark van der Berg",
        ] {
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
