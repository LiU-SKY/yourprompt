//! What a prompt is, decided before anything is said about how well it is
//! written.
//!
//! Every prompt used to be graded, and a working session is full of prompts
//! that are not requests. "continue" scored 0.0, "yes" 15.1 and "ok do it"
//! 24.5, and each left an F in the status line in place of the task it was
//! continuing. `/score` reached the hook before Claude Code expanded it, was
//! graded 15.1, and was then what `/score` explained. A request that scores
//! 633.9 in English scored 149.1 in Spanish, because only English and Korean
//! can be read. And Claude Code runs the prompt hook on turns it starts by
//! itself: a background agent's report replaced a 799.8 A- with its own
//! 622.1 B-, which `/score` then explained.
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
    /// A turn Claude Code started on its own, wrapped in its own markup: a
    /// background task reporting back in a `<task-notification>`, or a
    /// message from another session. Nobody typed it, so it says nothing
    /// about how the user writes prompts.
    Automated,
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
/// since the deploy". A command's argument rarely opens with a verb in the
/// third person, and "/checkout takes 8 seconds to load" was skipped as a
/// command for want of "takes". Sorted, for binary search.
const PATH_VERBS: &[&str] = &[
    "accepts",
    "breaks",
    "broke",
    "crashed",
    "crashes",
    "errors",
    "failed",
    "fails",
    "flickers",
    "freezes",
    "gave",
    "gives",
    "hangs",
    "hung",
    "ignores",
    "keeps",
    "leaks",
    "loads",
    "needs",
    "panics",
    "redirected",
    "redirects",
    "rejects",
    "renders",
    "requires",
    "responded",
    "responds",
    "returned",
    "returns",
    "sends",
    "sets",
    "shows",
    "stalls",
    "stopped",
    "stops",
    "submits",
    "takes",
    "threw",
    "throws",
    "timed",
    "times",
    "triggers",
    "updates",
    "works",
];

/// Nouns that make `/name` the start of a noun phrase -- "/login page",
/// "/users endpoint" -- when a verb follows within a word of them: "/login
/// page flickers on load", "/login request body is not validated". Each such
/// sentence was skipped as a command. Alone, or with no verb behind it, the
/// noun is an argument: "/test api". Sorted, for binary search.
const ROUTE_NOUNS: &[&str] = &[
    "api",
    "body",
    "button",
    "call",
    "component",
    "controller",
    "cookie",
    "dir",
    "directory",
    "disk",
    "endpoint",
    "flow",
    "folder",
    "form",
    "handler",
    "link",
    "logic",
    "middleware",
    "modal",
    "mount",
    "page",
    "partition",
    "query",
    "request",
    "response",
    "route",
    "screen",
    "service",
    "session",
    "template",
    "test",
    "tests",
    "token",
    "url",
    "view",
    "volume",
    "webhook",
];

/// Adverbs that do the same when a verb follows them: "/cart never updates
/// the badge count", "/login still fails after the fix". Sorted, for binary
/// search.
const ROUTE_ADVERBS: &[&str] = &[
    "also",
    "always",
    "constantly",
    "frequently",
    "intermittently",
    "just",
    "never",
    "now",
    "occasionally",
    "often",
    "only",
    "randomly",
    "rarely",
    "sometimes",
    "sporadically",
    "still",
    "suddenly",
    "usually",
];

/// Words that, after an auxiliary, make it the start of a question: "is
/// this", "should we". A question is what a command can take, and typed
/// without its question mark "/review is this safe to merge" was graded,
/// 374.8, where the same with one was not. Sorted, for binary search.
const QUESTION_SUBJECTS: &[&str] = &[
    "he", "i", "it", "she", "that", "there", "these", "they", "this", "those", "we", "you",
];

/// Words that, after a status code, make it a route's: "/sign-up 500 on
/// submit". After a hyphenated name a bare code is an issue number,
/// "/fix-issue 201 and add a regression test". Sorted, for binary search.
const STATUS_CONTEXT: &[&str] = &[
    "after", "error", "errors", "on", "since", "when", "whenever",
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
/// route; "/fix-issue 123" passes an issue number. An unassigned code is
/// never a status.
fn is_status_code(word: &str) -> bool {
    let Ok(code) = word.parse::<u16>() else {
        return false;
    };
    word.len() == 3
        && matches!(code,
            100..=103 | 200..=208 | 226 | 300..=308 | 400..=418 | 421..=426 | 428 | 429
            | 431 | 451 | 500..=508 | 510 | 511)
}

/// True for a word that says what a route does: a listed verb, or a status
/// code used as one, "/signup 500s when the email already exists".
fn is_path_verb(word: &str) -> bool {
    PATH_VERBS.binary_search(&word).is_ok() || word.strip_suffix('s').is_some_and(is_status_code)
}

/// A verb or an auxiliary: what makes the words in front of it a subject.
fn is_finite(word: &str) -> bool {
    is_path_verb(word) || PATH_AUXILIARIES.binary_search(&word).is_ok()
}

/// A word as the rules here compare it: lowercased, with a straight
/// apostrophe, and without the punctuation that ends a clause. An arrow is
/// all punctuation, and is kept whole.
fn normalise(word: &str) -> String {
    let trimmed = word.trim_end_matches(|c: char| c.is_ascii_punctuation() && c != '\'');
    let kept = if trimmed.is_empty() { word } else { trimmed };
    kept.replace('\u{2019}', "'").to_ascii_lowercase()
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
    // comment, which Claude Code sends to the model. It never ends in a
    // colon, while a path quoted from an error message does: "/tmp: No space
    // left on device".
    let named = name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'-'));
    if !named || name.ends_with(':') || ROOT_DIRS.binary_search(&name).is_ok() {
        return false;
    }
    let after: Vec<String> = words.take(3).map(normalise).collect();
    let Some(next) = after.first() else {
        return true;
    };
    if name.eq_ignore_ascii_case(SIDE_QUESTION) {
        return true;
    }
    if is_path_verb(next) {
        return false;
    }
    if PATH_AUXILIARIES.binary_search(&next.as_str()).is_ok() {
        return asks(text)
            || after
                .get(1)
                .is_some_and(|w| QUESTION_SUBJECTS.binary_search(&w.as_str()).is_ok());
    }
    // Plugin and MCP commands are namespaced, "/review:security" and
    // "/mcp__github__list_prs", and no route is.
    if name.contains([':', '_']) {
        return true;
    }
    if is_status_code(next) {
        return name.contains('-')
            && !after
                .get(1)
                .is_some_and(|w| STATUS_CONTEXT.binary_search(&w.as_str()).is_ok());
    }
    !about_the_route(&after)
}

/// True when the words after `/name` read as a sentence with the route as
/// its subject: "page flickers", "never updates", "-> 500", "and /logout
/// both need CSRF tokens".
fn about_the_route(after: &[String]) -> bool {
    let word = |i: usize| after.get(i).map_or("", String::as_str);
    let first = word(0);
    (ROUTE_NOUNS.binary_search(&first).is_ok() && (is_finite(word(1)) || is_finite(word(2))))
        || (ROUTE_ADVERBS.binary_search(&first).is_ok() && is_finite(word(1)))
        || (matches!(first, "->" | "=>" | "\u{2192}") && is_status_code(word(1)))
        || (matches!(first, "and" | "or" | "&") && word(1).starts_with('/'))
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

/// The elements Claude Code is known to wrap around what it sends on its
/// own: a background task reports back in a `<task-notification>`, and a
/// `<system-reminder>` carries what Claude Code adds to a turn. Any other
/// lowercase hyphenated tag counts as well, since that is the shape of
/// Claude Code's markup and a message from another session may arrive in
/// one.
const HARNESS_TAGS: &[&str] = &["system-reminder", "task-notification"];

/// True when the text is harness markup and nothing else: one element or
/// more, each opened by `<tag` and closed by `</tag>`, with only whitespace
/// around and between them.
///
/// The close tag has to end the prompt. "<my-button> doesn't render, fix
/// it", markup pasted ahead of a question, and a prompt that opens with
/// `<div>` are the user's own, and graded. So is pasted text that Claude
/// Code marks as `<pasted_content id="1">`: its tag is not hyphenated.
fn is_automated(text: &str) -> bool {
    let mut rest = text.trim();
    if rest.is_empty() {
        return false;
    }
    while !rest.is_empty() {
        let Some(open) = rest.strip_prefix('<') else {
            return false;
        };
        let length = open
            .find(|c: char| !(c.is_ascii_lowercase() || c == '-'))
            .unwrap_or(open.len());
        let (name, after) = open.split_at(length);
        let hyphenated = name.contains('-') && name.split('-').all(|part| !part.is_empty());
        if !(hyphenated || HARNESS_TAGS.contains(&name))
            || !after.starts_with(|c: char| c == '>' || c.is_whitespace())
        {
            return false;
        }
        let close = format!("</{name}>");
        let Some(end) = after.find(&close) else {
            return false;
        };
        if !after[..end].contains('>') {
            return false;
        }
        rest = after[end + close.len()..].trim_start();
    }
    true
}

/// Classify a prompt, naming its language when it is [`Kind::Unsupported`]
/// and the evidence points at one.
///
/// This reads all of the text it is given. The hook and the score read the
/// instruction instead, through [`crate::kind_of`]: pasted blocks cut out,
/// and only its first [`crate::prompt::MAX_INSTRUCTION_BYTES`]. The cut
/// changes nothing here, since the marker left where a paste was is code
/// just as the paste was. The cap can: past 64 KB this reads text the score
/// never sees, and the two may disagree.
pub fn kind_of(text: &str) -> (Kind, Option<&'static str>) {
    classify(text, &yp_lang::tokenize(text))
}

/// [`kind_of`] for a caller that has already tokenized the text.
pub(crate) fn classify(text: &str, tokens: &[Token]) -> (Kind, Option<&'static str>) {
    if is_automated(text) {
        return (Kind::Automated, None);
    }
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

    /// What the hook decides: [`crate::kind_of`], which reads the prompt the
    /// way the score reads it, rather than the raw [`kind_of`] above.
    fn classified(text: &str) -> (Kind, Option<&'static str>) {
        crate::kind_of(text)
    }

    fn kind(text: &str) -> Kind {
        classified(text).0
    }

    #[test]
    fn the_word_lists_are_sorted_for_binary_search() {
        for list in [
            ROOT_DIRS,
            PATH_VERBS,
            ROUTE_NOUNS,
            ROUTE_ADVERBS,
            QUESTION_SUBJECTS,
            STATUS_CONTEXT,
            PATH_AUXILIARIES,
        ] {
            assert!(list.windows(2).all(|w| w[0] < w[1]), "{list:?}");
        }
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
            "/fix-issue 201 and add a regression test",
            // A question is what some commands take, and the side question
            // takes nothing else. Its word order makes it one without the
            // question mark.
            "/review is this safe to merge?",
            "/review is this safe to merge",
            "/review does this handle the empty cart case",
            "/review should we split this PR",
            "/btw is the cache per user?",
            "/btw does this leak on retry?",
            "/btw is this thread-safe",
            "/explain why this is slow",
            // A noun is an argument unless a verb follows it, and a
            // namespaced command takes any words at all.
            "/test api",
            "/write-tests handler",
            "/frontend:component Button with a loading state",
            "/mcp__linear__create_issue login is broken on Safari",
            "/add-dir /tmp/scratch",
            "/memory add always run cargo fmt before committing",
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
            "/checkout takes 8 seconds to load",
            "/checkout times out with more than 3 items",
            "/upload rejects files over 10MB, raise the limit to 50MB",
            "/signup 500s when the email already exists",
            "/register needs rate limiting",
            "/health endpoint returns 404 behind the load balancer",
            "/users endpoint needs pagination",
            "/admin route needs an auth guard",
            "/login page flickers on load",
            "/login request body is not validated",
            "/data volume is full on the staging box",
            "/cart never updates the badge count",
            "/login sometimes fails with a 502",
            "/login randomly 500s",
            "/login 404",
            "/login -> 500",
            "/sign-up 500 on submit",
            "/login and /logout both need CSRF tokens",
            "/tmp: No space left on device",
            "/app: permission denied when the container starts",
        ] {
            assert_eq!(kind(text), Kind::Task, "{text:?}");
        }
    }

    #[test]
    fn a_turn_claude_code_sent_is_automated() {
        // A background agent reporting back replaced a 799.8 A- in the
        // status line with its own 622.1 B-.
        for text in [
            "<task-notification>\n<task-id>agent-a1b</task-id>\n<status>completed</status>\n\
             <summary>Agent \"Investigate auth bug\" finished</summary>\n<result>Found null \
             pointer in src/auth/validate.ts:42.</result>\n</task-notification>",
            "<system-reminder>\nThe user opened src/cli.rs.\n</system-reminder>",
            "  <system-reminder>a</system-reminder>\n\n<task-notification>b</task-notification>\n",
            "<cross-session-message from=\"api-worker\">Schema migration finished\
             </cross-session-message>",
        ] {
            assert_eq!(classified(text), (Kind::Automated, None), "{text:?}");
            // The score says so too, so `yp score` and the web page agree
            // with the hook.
            assert_eq!(
                crate::score(text).unwrap().kind,
                Kind::Automated,
                "{text:?}"
            );
        }
        // Long enough that its result is cut out as pasted, the report is
        // still read as one.
        let log = "error[E0308]: mismatched types\n".repeat(40);
        let long = format!(
            "<task-notification>\n<result>\n```\n{log}```\n</result>\n</task-notification>"
        );
        assert_eq!(kind(&long), Kind::Automated);
    }

    #[test]
    fn markup_the_user_wrote_is_still_a_task() {
        for text in [
            "<my-button> doesn't render, fix it",
            "<my-card>\n  <h2>Title</h2>\n</my-card>\nwhy is the title not bold?",
            "<div class=\"card\">\n  <h2>Title</h2>\n</div>\nwhy is the title not bold?",
            "<div>hello</div>",
            "<task-notification>done</task-notification>\nwhy did the agent stop there?",
            "<system-reminder> was injected twice into the transcript, find out why",
            "<system-reminder>never closed",
            "<pasted_content id=\"1\">\nTraceback (most recent call last):\n</pasted_content id=\"1\">",
            "<Task-Notification>x</Task-Notification>",
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
            // Each of these was graded, almost all of them an F.
            "looks good to me",
            "sounds good to me",
            "please do",
            "yes please do",
            "let's do it",
            "let's continue",
            "let's go with option 2",
            "let's go with b",
            "I'd go with option 2",
            "yes to both",
            "thank you so much",
            "thanks a lot",
            "it worked",
            "works now",
            "that fixed it",
            "fixed",
            "the first option",
            "option two",
            "second",
            "the latter",
            "all of them",
            "next step",
            "go to the next step",
            "proceed to step 2",
            "move on",
            "skip",
            "that's it",
            "that's all",
            "all good",
            "I agree",
            "up to you",
            "your call",
            "either is fine",
            "whatever you think is best",
            "keep going until you're done",
            "hang on a sec",
            "actually, never mind",
            "forget it",
            "ok~",
            "haha ok",
            "👍",
            "✅",
            "🙏",
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
            // Each of these was graded, an F.
            "알아서 해줘",
            "넘어가자",
            "잠깐만요",
            "잠시만요",
            "다시 한번",
            "한 번만 더",
            "다 해줘",
            "전부",
            "모두 해줘",
            "완료",
            "해결됐어",
            "끝났어",
            "아냐",
            "아니오",
            "완벽해요",
            "최고야",
            "괜찮네",
            "됐다",
            "b로 해줘",
            "두 번째 걸로 해줘",
            "2번 방식으로",
            "ㄱㄱㄱ",
            "진행 ㄱ",
            "이대로 진행해",
            "끝까지 진행해줘",
            "쭉 진행해",
            "다음 단계 진행해",
            "진행시켜주세요",
            "오키도키",
            // "그렇게" points at a manner, not at a thing: this approves the
            // plan just proposed, as "yes, do it" does.
            "그렇게 해줘",
            "응 그렇게 해줘",
            "네 그렇게 진행해주세요",
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
            "완료?",
            "correct the typo in the README",
            "option e",
            "proceed with the migration",
            "continue where you left off in src/cli.rs",
            // A reply that leads into a request leads into a task.
            "let's fix the parser",
            "let's do that",
            "그렇게 고쳐줘",
            "알아서 고쳐줘",
            "ㅇㅇ 그렇게",
            "start the dev server",
            "skip the flaky tests",
            "the second test fails",
            "next, add docs",
            "and the tests?",
            "do the same for the other files",
            "it works on my machine, fix CI",
            "👍 now add a test",
            "다시 시작해줘",
            "요약해줘",
            "푸시 ㄱ",
            "아직도 안 돼",
            "한번 돌려봐",
            "다른 거 해줘",
        ] {
            assert_eq!(kind(text), Kind::Task, "{text:?}");
        }
    }

    #[test]
    fn saying_nothing_is_not_a_reply() {
        // Fillers, punctuation and emoji are skipped inside a reply, but on
        // their own they are graded as before -- and score near the floor.
        // An emoji that answers, "👍", is a reply like the word it stands
        // for; one that answers nothing is not.
        for text in ["", "   ", "hmm", "!!!", "🤔", "🐛", "haha", "음... 어..."] {
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
            classified(
                "arregla el bug en src/auth/login.rs para que verify_token devuelva \
                 Err(AuthError::Expired). Los tests deben pasar."
            ),
            (Kind::Unsupported, Some("Spanish"))
        );
        assert_eq!(
            classified(
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
        assert_eq!(classified("はい"), (Kind::Unsupported, Some("Japanese")));
        assert_eq!(
            classified("продолжай"),
            (Kind::Unsupported, Some("Russian"))
        );
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
                classified(text),
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
        for text in [
            "fix the bug",
            "/score",
            "yes",
            "그거 고쳐줘",
            "continue",
            "<system-reminder>x</system-reminder>",
        ] {
            assert_eq!(classified(text).1, None, "{text:?}");
        }
    }

    #[test]
    fn kinds_serialize_in_snake_case_and_read_back() {
        let kinds = [
            Kind::Task,
            Kind::Command,
            Kind::FollowUp,
            Kind::Unsupported,
            Kind::Automated,
        ];
        let names: Vec<String> = kinds
            .iter()
            .map(|k| serde_json::to_string(k).unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "\"task\"",
                "\"command\"",
                "\"follow_up\"",
                "\"unsupported\"",
                "\"automated\""
            ]
        );
        // A session sidecar stores the kind by this name and reads it back.
        for (kind, name) in kinds.iter().zip(&names) {
            assert_eq!(serde_json::from_str::<Kind>(name).unwrap(), *kind);
        }
        assert!(!Kind::Automated.is_graded());
    }
}
