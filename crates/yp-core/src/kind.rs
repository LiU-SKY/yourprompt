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

/// Names that call a command rather than a route: verbs of the work a
/// command does to code, and the nouns it is done on. Claude Code's own
/// `/bug`, `/review`, `/compact` and `/loop` take free text, and so do
/// custom commands such as `/fix`, `/debug`, `/plan` or `/tdd`, whose
/// argument is often a report that opens with what is broken: "/fix test
/// fails on windows", "/plan api returns too much data". Read as sentences
/// about a route, as "/login page flickers on load" is, 12 of 26 such
/// commands a reviewer typed were graded, from 87.2 to 280.1.
///
/// A route is named after what an application offers its users, and in the
/// plural when it lists them: `/login`, `/checkout`, `/users`, `/reviews`. A
/// command name that is also a common route, such as `/search`, `/upload`,
/// `/docs` or `/todo`, is left off. Sorted, for binary search.
const COMMAND_WORDS: &[&str] = &[
    "bug",
    "code-review",
    "commit",
    "compact",
    "debug",
    "diagnose",
    "explain",
    "fix",
    "fix-bug",
    "fix-issue",
    "implement",
    "investigate",
    "issue",
    "loop",
    "optimize",
    "plan",
    "pr",
    "refactor",
    "repro",
    "review",
    "review-pr",
    "security-review",
    "tdd",
    "test",
    "think",
    "troubleshoot",
    "ultrathink",
];

/// True for a name in [`COMMAND_WORDS`], however it is capitalised.
fn is_command_word(name: &str) -> bool {
    COMMAND_WORDS
        .binary_search(&name.to_ascii_lowercase().as_str())
        .is_ok()
}

/// Words that, after an auxiliary, make it the start of a question: "is
/// this", "should we". A question is what a command can take, and typed
/// without its question mark "/review is this safe to merge" was graded,
/// 374.8, where the same with one was not. Sorted, for binary search.
const QUESTION_SUBJECTS: &[&str] = &[
    "he", "i", "it", "she", "that", "there", "these", "they", "this", "those", "we", "you",
];

/// Words that open a noun phrase. After a command word and an auxiliary
/// they make a question too, "/review are the tests sufficient", graded
/// 109.6: no command is the subject of "are the tests". After a route's
/// name they do not, "/login is the slowest page". Sorted, for binary
/// search.
const DETERMINERS: &[&str] = &[
    "a", "all", "an", "any", "every", "my", "our", "some", "the", "their", "your",
];

/// Words that, after a status code, make it a route's: "/sign-up 500 on
/// submit". After a hyphenated name or a command word a bare code is an
/// issue or pull request number, "/fix-issue 201 and add a regression
/// test", "/review 404". Sorted, for binary search.
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

/// The name after the slash when the text opens the way a slash command
/// does, whatever follows it: "fix" in "/fix test fails on windows", and
/// "checkout" in "/checkout takes 8 seconds to load". A second `/` makes the
/// first word a path instead.
///
/// A command's name starts with a letter: "/-- docs -/" opens a Lean
/// comment, which Claude Code sends to the model. It never ends in a colon,
/// while a path quoted from an error message does: "/tmp: No space left on
/// device".
pub fn slash_name(text: &str) -> Option<&str> {
    let name = text.split_whitespace().next()?.strip_prefix('/')?;
    let named = name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'-'));
    (named && !name.ends_with(':')).then_some(name)
}

/// A slash command: the first word is `/` and a name, with no second `/`
/// that would make it a path. `/review:security` is a command;
/// `/usr/local/bin is not on PATH` is not, and neither is a prompt about a
/// root directory or a route: "/tmp is full", "/health returns 500".
fn is_command(text: &str) -> bool {
    let Some(name) = slash_name(text) else {
        return false;
    };
    if ROOT_DIRS.binary_search(&name).is_ok() {
        return false;
    }
    let after: Vec<String> = text
        .split_whitespace()
        .skip(1)
        .take(3)
        .map(normalise)
        .collect();
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
            || after.get(1).is_some_and(|w| {
                QUESTION_SUBJECTS.binary_search(&w.as_str()).is_ok()
                    || (is_command_word(name) && DETERMINERS.binary_search(&w.as_str()).is_ok())
            });
    }
    // Plugin and MCP commands are namespaced, "/review:security" and
    // "/mcp__github__list_prs", and no route is.
    if name.contains([':', '_']) {
        return true;
    }
    if is_status_code(next) {
        return (name.contains('-') || is_command_word(name))
            && !after
                .get(1)
                .is_some_and(|w| STATUS_CONTEXT.binary_search(&w.as_str()).is_ok());
    }
    !about_the_route(name, &after)
}

/// True when the words after `/name` read as a sentence with the route as
/// its subject: "page flickers", "never updates", "-> 500", "and /logout
/// both need CSRF tokens".
///
/// After a command word, a noun or an adverb with a verb behind it is the
/// command's argument instead, a report of what is broken: "/fix test fails
/// on windows", "/fix still fails after the patch". A verb right after the
/// name still makes the name its subject, "/debug returns 404 in
/// production".
fn about_the_route(name: &str, after: &[String]) -> bool {
    let word = |i: usize| after.get(i).map_or("", String::as_str);
    let first = word(0);
    let subject = (ROUTE_NOUNS.binary_search(&first).is_ok()
        && (is_finite(word(1)) || is_finite(word(2))))
        || (ROUTE_ADVERBS.binary_search(&first).is_ok() && is_finite(word(1)));
    (subject && !is_command_word(name))
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

/// The elements Claude Code wraps around a turn it sends on its own, by name.
///
/// The prompt hook also runs when a background task reports back, in a
/// `<task-notification>`, and when another session sends a message, in a
/// `<cross-session-message>`. A `<system-reminder>` carries what Claude Code
/// adds to a turn. The rest are the markup Claude Code writes around a slash
/// command, a local command's output, `!` shell input and a prompt hook's
/// output.
///
/// Any lowercase hyphenated tag used to count as well, and that is also how
/// people structure a prompt of their own: all eight a reviewer wrote only
/// in tags such as `<task-description>`, `<bug-report>` and `<error-log>`
/// were skipped as Claude Code's, where they scored from 190.6 to 754.7. A
/// turn wrapped in a tag missing here is graded, which costs an F in the
/// status line; a request taken for one is never seen. Sorted, for binary
/// search.
const HARNESS_TAGS: &[&str] = &[
    "bash-input",
    "bash-stderr",
    "bash-stdout",
    "command-args",
    "command-message",
    "command-name",
    "cross-session-message",
    "local-command-caveat",
    "local-command-stderr",
    "local-command-stdout",
    "system-reminder",
    "task-notification",
    "user-prompt-submit-hook",
];

/// True when the text is harness markup and nothing else: one element or
/// more, each a [`HARNESS_TAGS`] element opened by `<tag` and closed by
/// `</tag>`, with only whitespace around and between them. What an element
/// holds is not read, so a report may carry markup of any kind.
///
/// The close tag has to end the prompt. "<my-button> doesn't render, fix
/// it", markup pasted ahead of a question, and a prompt that opens with
/// `<div>` are the user's own, and graded. So is pasted text that Claude
/// Code marks as `<pasted_content id="1">`, and a request wrapped in a tag of
/// the user's own, `<bug-report>`.
///
/// One pass, each byte read a bounded number of times, so this can read a
/// prompt whole: given a megabyte of a report, of reminders or of nested
/// markup, the whole hook takes 8 to 14 ms.
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
        if HARNESS_TAGS.binary_search(&name).is_err()
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
/// never sees, and the two may disagree, except on whether Claude Code sent
/// the turn, which both decide on all of it.
pub fn kind_of(text: &str) -> (Kind, Option<&'static str>) {
    classify(text, text, &yp_lang::tokenize(text))
}

/// [`kind_of`] for a caller that has already tokenized the text it reads,
/// `read`, the start of `whole`.
///
/// Whether Claude Code sent the turn is decided on `whole`. Its markup is
/// recognised by the close tag that ends it, and a report longer than the
/// cap lost that tag to it: a task notification of 69,326 bytes was graded
/// 205.7, an F, and took the place of an 809.5 A in the status line.
pub(crate) fn classify(whole: &str, read: &str, tokens: &[Token]) -> (Kind, Option<&'static str>) {
    if is_automated(whole) {
        return (Kind::Automated, None);
    }
    if is_command(read) {
        return (Kind::Command, None);
    }
    if yp_lang::resources().is_some_and(|r| r.follow_ups.is_follow_up(tokens)) {
        return (Kind::FollowUp, None);
    }
    if let Some(foreign) = yp_lang::foreign::detect(read) {
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
            COMMAND_WORDS,
            QUESTION_SUBJECTS,
            DETERMINERS,
            STATUS_CONTEXT,
            PATH_AUXILIARIES,
            HARNESS_TAGS,
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
    fn a_command_word_keeps_its_argument_however_it_reads() {
        // A report of what is broken opens the way a sentence about a route
        // does. Each of these was graded, an F.
        for text in [
            "/debug test fails on CI",
            "/fix test fails on windows",
            "/tdd form submits twice",
            "/bug button renders twice on hover",
            "/explain query is slow",
            "/plan api returns too much data",
            "/issue token refresh is broken",
            "/investigate request times out after 30s",
            "/bug view shows stale data after save",
            "/fix webhook handler panics on empty body",
            "/fix tests are flaky on macOS",
            "/ultrathink session handling is wrong",
            "/Fix test fails on windows",
            "/fix still fails after the patch",
            // A question needs no question mark, and after a command word
            // "are the tests" opens one: no command is its subject.
            "/review are the tests sufficient",
            "/review has the migration been tested",
            "/review is the cache invalidated on logout",
            // A bare code after a command word is an issue or a pull
            // request, as it is after "/fix-issue".
            "/review 404",
            "/fix 500",
        ] {
            assert_eq!(kind(text), Kind::Command, "{text:?}");
        }
        // The name is still the subject of a verb that follows it right
        // away, of an auxiliary that does not open a question, of a status
        // code told where it happens, and of a sentence about two names.
        for text in [
            "/debug returns 404 in production",
            "/review is broken",
            "/fix 500 on submit",
            "/fix and /test both fail on windows",
            "/plan -> 500",
        ] {
            assert_eq!(kind(text), Kind::Task, "{text:?}");
        }
        // A route with the same report is a task, as before.
        for text in [
            "/login test fails on windows",
            "/checkout form submits twice",
            "/search api returns too much data",
            "/reviews page is the slowest",
        ] {
            assert_eq!(kind(text), Kind::Task, "{text:?}");
        }
    }

    #[test]
    fn a_slash_name_is_what_a_command_would_be_called() {
        for (text, name) in [
            ("/fix test fails on windows", "fix"),
            ("  /review:security", "review:security"),
            ("/checkout takes 8 seconds to load", "checkout"),
            ("/fix-issue 123", "fix-issue"),
        ] {
            assert_eq!(slash_name(text), Some(name), "{text:?}");
        }
        for text in [
            "/usr/local/bin is not on PATH",
            "/tmp: No space left on device",
            "/-- The main theorem about lists -/ fails to compile",
            "fix the /score command",
            "/",
            "/점수",
        ] {
            assert_eq!(slash_name(text), None, "{text:?}");
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
            "<command-name>/compact</command-name>\n<command-message>compact</command-message>\n\
             <command-args></command-args>",
            "<local-command-stdout>Compacted. ctrl+o to see full summary</local-command-stdout>",
            "<bash-input>git status</bash-input>",
            "<bash-stdout>On branch main\nnothing to commit</bash-stdout><bash-stderr></bash-stderr>",
            "<user-prompt-submit-hook>Prompt blocked by policy</user-prompt-submit-hook>",
            // What a report holds is not read: the user's markup may be in it.
            "<task-notification>\n<result>\n<bug-report>The form submits twice.</bug-report>\n\
             <my-button>Save</my-button>\n</result>\n</task-notification>",
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
            // A request wrapped in tags of the user's own, hyphenated as
            // Claude Code's are. Each of these was skipped as a turn Claude
            // Code sent, though they scored from 190.6 to 754.7.
            "<task-description>\nFix verify_token in src/auth/login.rs so it returns \
             Err(AuthError::Expired) instead of panicking on an expired token.\n</task-description>",
            "<coding-guidelines>\nUse snake_case and keep functions under 40 lines.\n\
             </coding-guidelines>\n<user-request>\nRename parseArgs to parse_args in src/cli.rs \
             and update every caller.\n</user-request>",
            "<task-context>\nWe use Postgres 16 and sqlx.\n</task-context>\n\n<task-instructions>\n\
             Add a migration that creates the audit_log table with id, user_id, action and \
             created_at columns.\n</task-instructions>\n\n<acceptance-criteria>\ncargo sqlx \
             migrate run succeeds and cargo test passes.\n</acceptance-criteria>",
            "<bug-report>\nThe login form submits twice when you press Enter. It happens in \
             Chrome and Firefox, not Safari.\n</bug-report>",
            "<error-log>\nthread 'main' panicked at src/auth/login.rs:42:9:\ncalled \
             `Option::unwrap()` on a `None` value\n</error-log>\n<expected-behavior>\n\
             verify_token returns Err(AuthError::Expired)\n</expected-behavior>",
            "<user-story>As a user I want to reset my password from the login page</user-story>",
            "<code-review-request>\nReview src/parser.rs for panics on malformed input.\n\
             </code-review-request>",
            "<feature-request>Add a --dry-run flag to yp install that prints the settings diff \
             without writing it.</feature-request>",
            "<bug-report>\nverify_token in src/auth/login.rs panics on an expired token. It \
             should return Err(AuthError::Expired). tests/auth.rs must still pass.\n</bug-report>",
            "<user-request>\nAdd a dark mode toggle to the settings page and remember the choice \
             in localStorage.\n</user-request>",
            "<coding-task>Fix verify_token so it returns Err(AuthError::Expired) instead of \
             panicking.</coding-task>",
            // Beside Claude Code's own markup, the user's still makes the
            // prompt theirs.
            "<system-reminder>The user opened src/cli.rs.</system-reminder>\n\
             <bug-report>The lint fails on this file.</bug-report>",
        ] {
            assert_eq!(kind(text), Kind::Task, "{text:?}");
        }
    }

    /// A background agent's report: `sentences` sentences of result in a
    /// `<task-notification>`, the close tag at the very end.
    fn report(sentences: usize) -> String {
        format!(
            "<task-notification>\n<task-id>a1b2</task-id>\n<status>completed</status>\n\
             <summary>Agent \"Investigate auth bug\" finished</summary>\n<result>{}</result>\n\
             </task-notification>",
            "The agent read src/auth/login.rs and found the bug. ".repeat(sentences)
        )
    }

    #[test]
    fn a_report_longer_than_what_is_scored_is_still_automated() {
        // Only the first 64 KB of an instruction is scored, and the close tag
        // that marks a report as Claude Code's was cut off with the rest: a
        // report of 69,326 bytes was graded 205.7, an F, and recorded in
        // place of the user's task.
        let long = report(1_400);
        assert!(long.len() > 70 * 1024, "{} bytes", long.len());
        assert!(crate::prompt::split(&long).capped().is_some());
        assert_eq!(classified(&long), (Kind::Automated, None));
        assert_eq!(crate::score(&long).unwrap().kind, Kind::Automated);
        // A megabyte of one.
        assert_eq!(kind(&report(20_000)), Kind::Automated);
        // As long, the user's own markup and a report with a question after
        // it are graded, as they are when short.
        let theirs = format!(
            "<bug-report>\n{}</bug-report>",
            "The login form submits twice when you press Enter. ".repeat(1_400)
        );
        assert_eq!(kind(&theirs), Kind::Task);
        assert_eq!(crate::score(&theirs).unwrap().kind, Kind::Task);
        assert_eq!(kind(&format!("{long}\nwhy did it stop there?")), Kind::Task);
    }

    #[test]
    fn markup_is_decided_in_time_that_grows_with_its_length() {
        // The whole of a prompt is read for its markup, so the work has to
        // grow no faster than the prompt does. Each shape is a megabyte,
        // timed against a megabyte of prose.
        fn time(text: &str) -> std::time::Duration {
            (0..3)
                .map(|_| {
                    let start = std::time::Instant::now();
                    std::hint::black_box(crate::kind_of(text));
                    start.elapsed()
                })
                .min()
                .unwrap()
        }
        const MB: usize = 1 << 20;
        let fill = |unit: &str| unit.repeat(MB / unit.len());
        let plain = time(&fill(
            "The agent read src/auth/login.rs and found the bug. ",
        ));
        for text in [
            report(20_000),
            fill("<system-reminder>x</system-reminder>\n"),
            format!(
                "<task-notification>\n<result>\n{}</result>\n</task-notification>",
                fill("<x-y><a-b>z</a-b></x-y>\n")
            ),
            fill("<task-notification>"),
            format!("{} fix it", fill("<system-reminder>x</system-reminder>")),
            format!("<system-reminder>{}", fill("</system-reminde")),
        ] {
            let markup = time(&text);
            assert!(
                markup < plain * 20 + std::time::Duration::from_millis(100),
                "{:.40}: {markup:?}, against {plain:?} for prose",
                text
            );
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
            // A plan approved without naming work: 65.0 and 87.2.
            "proceed as planned",
            "do as you suggested",
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
            // Each of these was graded, an F: "the rest" and "where you
            // left off", and thanks as a session ends.
            "마저 해줘",
            "이어서 해줘",
            "수고하셨어요",
            "고생하셨습니다",
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
            // A plan approved with an action verb is work, as "fix it" is,
            // and so is an approval with a request in it, or a lead with
            // nothing after it.
            "implement the plan",
            "go ahead and implement it",
            "proceed as planned but skip the flaky tests",
            "마저",
            "이어서 로그인 버그 고쳐줘",
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
