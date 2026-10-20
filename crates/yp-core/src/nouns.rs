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
//! Terse reports drop the article, and "build is broken" kept the old reading,
//! at 305.4 as high as "fix the bug"; "deploy failed last night" scored 356.4.
//! An action word directly followed by a verb is that verb's subject, so a
//! noun, wherever it stands: "docker build fails", "When DEBUG is True". The
//! verbs are [`FINITE`]: copulas, auxiliaries and modals, and the few finite
//! verbs that are never a plural noun an instruction could take as its object.
//! "fails" is one; "crashes" is not, because "fix crashes on startup" asks for
//! a fix. The second word after a determiner is a noun too, the head of a
//! compound: "test" in "fix the login test", "fix" in "the proposed fix". Not
//! when the word between cannot modify a noun ("the user can test it", "a way
//! to build it"), and not after a verb that takes a bare verb after its object
//! ("make the parser log errors", "let the user test it"). In SWE-bench
//! Lite's 300 issues these two readings take 20 action words for nouns, and
//! every one is a noun ("the proposed fix", "When DEBUG is True"). In
//! HumanEvalComm they take one, the "move" of "All cars move in the same
//! speed", which asks for nothing either way. A plural that "of" follows is
//! not one of those verbs, since none of them takes "of": "Document returns
//! of parse_args" and "Investigate causes of the leak" ask for what they say.
//! Neither is "returns" before "in", as in "Fix returns in parse_args". Read
//! as the verb, "returns" took the objective from both, which scored 270.3
//! rather than 424.2.
//!
//! An action word joined by a hyphen to "pre", "post" or "non" is part of a
//! modifier: the "pre-commit" hook, a "post-install" script, "non-test"
//! files. Once commit, push and install were actions, "Add a pre-commit hook
//! that runs prettier on staged files" was read as two objectives. Any other
//! compound is read as one word where it starts, so "the re-deploy step" is a
//! noun and "Re-deploy staging" a verb, and a flag such as "--dry-run" is a
//! name. The object of a preposition is a noun as well: "on push", "before
//! commit", "during build". "Trigger the deploy on push to main" took "push"
//! as its objective, and "use SELECT ... FOR UPDATE" took "UPDATE". In
//! SWE-bench Lite's 300 issues an action word follows a preposition 42 times
//! outside code, 34 of them in the frames of a traceback ("in execute"), and
//! every one is a noun. HumanEvalComm has none.
//!
//! A word inside a name is part of the name, by the test the passes share,
//! [`crate::antecedents::is_name`]: "test" in `src/test/foo.rs`, "build" in
//! `build()` and `RepoIndex::build`, "execute" in `cursor.execute`. "fix the
//! panic in src/test/foo.rs" was read as two objectives. SWE-bench Lite's
//! issues hold 151 action words inside paths, dotted names and calls, and
//! HumanEvalComm's problems 31 in their signatures and examples (`def
//! add(x: int, y: int)`), every one of them a name.
//!
//! "to" and an action word after the object of another one is that action's
//! purpose, not a second request: "update the deploy script to run migrations"
//! asks for one thing, and kept 29.6 of the 40 singularity points; "change the
//! log level to debug" names a level. A clause boundary or a joining word
//! between them ends that reading: "fix the parser and try to refactor the
//! lexer" asks for two things. And a tool's subcommand that another action
//! runs names the command rather than asking for more: the "install" of "run
//! npm install"; see [`TOOLS`].
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
//! A Korean verb carries its ending, 추가해 or 고쳐줘, which no noun has. The
//! loanwords the Korean lexicon also lists as actions (리팩터링, 마이그레이션,
//! 업데이트) are nouns that become verbs with 하다 or 시키다 after them. With
//! neither, and another action after them in the sentence, they are that
//! action's object or its modifier: "업데이트 버튼 고쳐줘" fixes the update
//! button and was read as two objectives, as "마이그레이션 파일 추가해줘" was.
//! With nothing after them to do, they keep their reading, so "리팩터링
//! 부탁해" still asks for one. A Korean action word ending in -도록 is a
//! purpose, as "to" and a verb is in English: "마이그레이션을 실행하도록 배포
//! 스크립트를 수정해줘" asks for one change, and was read as three.
//!
//! A Korean clause that says when something happens, rather than asks for
//! it, is the twin of "on push": push 할 때마다, 리팩터링할 때, merge 되면,
//! 커밋하기 전에, 업데이트 시. "push 할 때마다 cargo test 랑 cargo clippy
//! 돌리는 GitHub Actions 워크플로 만들어줘" was read as three objectives, push
//! among them. A request after 하면 still asks for what it names: 리팩터링하면
//! 좋겠어. And a tool's subcommand that the Korean verb after it runs names
//! the command, as it does after "run": "cargo test 돌려줘" counted test and
//! 돌려 as two objectives, where "run cargo test" counts one.
//!
//! "Return" opens an instruction as "fix" does, but it is not an action verb,
//! because "Make verify_token return Err(...)" would then ask for two things.
//! So "Return Err(AuthError::Expired) from verify_token instead of
//! panicking." had no objective and scored 180.1, against 564.7 for the
//! "Make ..." form. When nothing else in a prompt is the action, a "return"
//! that opens a sentence or a clause is: "If the input is empty, return an
//! empty list." A "return" inside a result-shape cue ("should return", "return
//! value") stays that cue, and the Korean 반환해줘 becomes the action in place
//! of its result shape, so neither language counts one word twice.
//!
//! The same reading keeps a vague verb's charge off a noun. The polysemous
//! verbs are charged as smells, and "check" in "fix the type check errors"
//! cost 46.8 points, "check" in "the health check fails" 64.6. And "that"
//! is charged as a vague pronoun only when it points, as "this" does; see
//! [`points`].
//!
//! Grounding still reads such a noun as an instruction word rather than as a
//! name, as it does a forbidden verb, and an opening "Return" as one as well.
//! Its deixis reads the smells left here, so a "that" that does not point is
//! no reference to it either.

use yp_lang::{CueId, Hit, SmellHit, SmellId, Span, Token, TokenKind};

use crate::antecedents;

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

/// Verbs that make the word directly before them their subject: copulas,
/// auxiliaries and modals, and finite verbs that are not also plural nouns an
/// instruction takes as its object. "crashes", "errors", "hangs", "panics",
/// "uses" and "checks" are left out for "fix crashes on startup", "log errors",
/// "replace uses of foo" and "add checks for null". Sorted, for binary search.
const FINITE: &[&str] = &[
    "are",
    "aren't",
    "breaks",
    "broke",
    "came",
    "can",
    "can't",
    "cannot",
    "caused",
    "causes",
    "comes",
    "contains",
    "could",
    "couldn't",
    "crashed",
    "did",
    "didn't",
    "does",
    "doesn't",
    "errored",
    "exists",
    "failed",
    "fails",
    "gave",
    "gets",
    "gives",
    "goes",
    "got",
    "had",
    "hadn't",
    "happened",
    "happens",
    "has",
    "hasn't",
    "helped",
    "helps",
    "hung",
    "is",
    "isn't",
    "keeps",
    "kept",
    "looked",
    "looks",
    "made",
    "makes",
    "matters",
    "may",
    "means",
    "meant",
    "might",
    "must",
    "mustn't",
    "needs",
    "panicked",
    "passed",
    "passes",
    "raised",
    "raises",
    "ran",
    "returned",
    "returns",
    "runs",
    "seemed",
    "seems",
    "shall",
    "should",
    "shouldn't",
    "sounds",
    "started",
    "starts",
    "stopped",
    "stops",
    "takes",
    "threw",
    "throws",
    "timed",
    "took",
    "was",
    "wasn't",
    "went",
    "were",
    "weren't",
    "will",
    "won't",
    "worked",
    "works",
    "would",
    "wouldn't",
];

/// Words that cannot be the first half of a compound noun, so a determiner two
/// words back makes nothing a noun: "the user can test", "a way to build",
/// "all of test". Sorted, for binary search.
const BREAKS: &[&str] = &[
    "about", "after", "again", "already", "also", "always", "am", "and", "as", "at", "be", "been",
    "before", "being", "but", "by", "do", "don't", "even", "ever", "first", "for", "from", "in",
    "into", "just", "longer", "never", "nor", "not", "now", "of", "on", "only", "or", "so",
    "still", "then", "to", "too", "very", "with",
];

/// Verbs whose object takes a bare verb after it: "make the parser log
/// errors", "let the user test it", "help the team deploy". Sorted, for
/// binary search.
const CAUSATIVES: &[&str] = &[
    "have", "hear", "help", "helps", "let", "lets", "letting", "make", "makes", "making", "see",
    "watch",
];

/// Words that join one clause or request to the next, so that a "to" after
/// them starts something new: "fix the parser and try to refactor the lexer".
/// Sorted, for binary search.
const JOINS: &[&str] = &["also", "and", "but", "nor", "or", "plus", "then"];

/// Command-line tools whose subcommands are action words: `npm install`,
/// `cargo test`, `git push`. Run by another action word, the subcommand names
/// the command it runs: "run npm install and fix the errors" was read as three
/// objectives once "install" was an action. "go" is left out, for "go fix
/// it". Sorted, for binary search.
const TOOLS: &[&str] = &[
    "apt", "brew", "bun", "cargo", "deno", "docker", "dotnet", "gh", "git", "gradle", "helm",
    "kubectl", "mvn", "npm", "npx", "pip", "pip3", "pipx", "pnpm", "poetry", "uv", "yarn",
];

/// Prepositions, whose object an action word after them is: "on push",
/// "before commit", "for commit messages", the "in execute" of a pasted
/// traceback. "to" is not one, because "to run" is a verb, and neither are
/// "but", "except" and "than", which a verb can follow: "do nothing but fix
/// the parser". Sorted, for binary search.
const PREPOSITIONS: &[&str] = &[
    "about", "after", "at", "before", "by", "during", "for", "from", "in", "into", "of", "on",
    "onto", "over", "per", "since", "through", "under", "until", "upon", "via", "with", "within",
    "without",
];

/// Prefixes that make a word a hyphen joins to them a modifier, never a verb:
/// the "pre-commit" hook, a "post-install" script, "non-test" files. "re" is
/// not one, because "re-deploy" and "re-enable" are verbs as often as "re-run"
/// is. Sorted, for binary search.
const PREFIXES: &[&str] = &["non", "post", "pre"];

/// The Korean nouns the cue lexicon lists as actions, which take 하다 or
/// 시키다 to become verbs. Sorted, for binary search.
const KO_NOUN_ACTIONS: &[&str] = &[
    "동기화",
    "디버그",
    "디버깅",
    "디코딩",
    "래핑",
    "롤백",
    "리팩터링",
    "리팩토링",
    "마이그레이션",
    "문서화",
    "비활성화",
    "업그레이드",
    "업데이트",
    "인라인",
    "인코딩",
    "재시도",
    "정규화",
    "최적화",
    "컴파일",
    "포맷",
    "포팅",
    "활성화",
];

/// What makes a Korean action noun the verb when it comes right after it, or
/// after its particle: 업데이트해, 리팩터링을 해줘, 롤백시켜.
const KO_VERBALIZERS: &[&str] = &[
    "시켜", "시켰", "시키", "시킨", "시킬", "하", "한", "할", "함", "합", "했", "해",
];

/// Particles a Korean noun may carry before the word it is the object of.
const KO_PARTICLES: &[char] = &['가', '는', '도', '를', '만', '은', '을', '의', '이'];

/// Particles that join a Korean noun to the next as a list, not as its
/// modifier: 업데이트와 배포, 롤백이랑 재시작.
const KO_JOINED: &[&str] = &["과", "와", "랑", "이랑"];

/// Words between which a Korean noun stands in a list: 업데이트 그리고 배포.
const KO_JOINS: &[&str] = &["그리고", "또는", "및"];

/// What after a Korean action word makes it the verb of a clause that says
/// when something happens rather than asks for it: push 할 때마다, 리팩터링할
/// 때, merge 될 경우, 커밋하기 전에. [`KO_IF`] holds the conditions, which a
/// request can follow.
const KO_WHEN: &[&str] = &[
    "될 경우",
    "될 시",
    "될 때",
    "될때",
    "되는 경우",
    "하기 전",
    "하는 경우",
    "할 경우",
    "할 시",
    "할 때",
    "할때",
];

/// The conditional endings, "if it is done".
const KO_IF: &[&str] = &["되면", "하면"];

/// What after 하면 or 되면 makes the whole a request rather than a condition:
/// 리팩터링하면 좋겠어 asks for a refactor, push 하면 돼 for a push, and 하면
/// 안 될까 as politely. 하면 안 돼 forbids it, and asks for nothing.
const KO_IF_ASKS: &[&str] = &[
    "감사",
    "고마",
    "돼",
    "되",
    "될",
    "안 될까",
    "어때",
    "어떨",
    "좋겠",
    "좋아",
    "좋을",
];

/// The endings after 반환해 and 리턴해 that make them a request rather than a
/// result shape: 반환해줘, 리턴해 주세요. 반환해야 is what something must do,
/// as "must return" is, and stays the result it names.
const KO_ASKING: &[&str] = &[" 주세요", " 줘", "라", "주세요", "줘"];

/// The same after 반환 and 리턴 when 하다 follows them: 반환하세요.
const KO_ASKING_HA: &[&str] = &["하라", "하세요"];

/// Words before a "return" that leave it opening its clause: "and return
/// early", "just return None". Sorted, for binary search.
const RETURN_LEADS: &[&str] = &[
    "also",
    "always",
    "and",
    "else",
    "instead",
    "just",
    "only",
    "or",
    "otherwise",
    "please",
    "simply",
    "so",
    "then",
];

/// Words after which "that" opens a clause: "make sure that", "so that",
/// "assume that", "check that". Sorted, for binary search.
const THAT_HEADS: &[&str] = &[
    "agree",
    "agreed",
    "appear",
    "appeared",
    "appears",
    "argue",
    "argued",
    "argues",
    "assert",
    "asserted",
    "asserting",
    "asserts",
    "assume",
    "assumed",
    "assumes",
    "assuming",
    "aware",
    "believe",
    "believed",
    "believes",
    "check",
    "checked",
    "checking",
    "checks",
    "claim",
    "claimed",
    "claims",
    "confirm",
    "confirmed",
    "confirming",
    "confirms",
    "decide",
    "decided",
    "demonstrate",
    "demonstrates",
    "discover",
    "discovered",
    "documented",
    "doubt",
    "ensure",
    "ensures",
    "ensuring",
    "except",
    "expect",
    "expected",
    "expecting",
    "expects",
    "feel",
    "feels",
    "felt",
    "forget",
    "forgot",
    "found",
    "given",
    "guarantee",
    "guaranteed",
    "guarantees",
    "guess",
    "hope",
    "hoped",
    "hopes",
    "implies",
    "imply",
    "indicate",
    "indicates",
    "insist",
    "knew",
    "know",
    "knowing",
    "known",
    "knows",
    "learn",
    "learned",
    "mean",
    "meaning",
    "means",
    "meant",
    "mention",
    "mentioned",
    "mentions",
    "note",
    "noted",
    "notes",
    "notice",
    "noticed",
    "notices",
    "now",
    "observe",
    "observed",
    "prove",
    "proved",
    "proven",
    "proves",
    "provided",
    "providing",
    "realise",
    "realised",
    "realize",
    "realized",
    "realizes",
    "recommend",
    "remember",
    "report",
    "reported",
    "reports",
    "require",
    "required",
    "requires",
    "reveal",
    "reveals",
    "said",
    "say",
    "saying",
    "says",
    "seem",
    "seemed",
    "seems",
    "show",
    "showed",
    "shown",
    "shows",
    "so",
    "specified",
    "specifies",
    "specify",
    "state",
    "stated",
    "states",
    "such",
    "suggest",
    "suggested",
    "suggests",
    "suppose",
    "supposed",
    "sure",
    "tell",
    "tells",
    "test",
    "tested",
    "testing",
    "tests",
    "think",
    "thinks",
    "thought",
    "told",
    "understand",
    "understood",
    "validate",
    "validated",
    "validates",
    "verified",
    "verifies",
    "verify",
    "verifying",
    "warn",
    "warned",
    "warns",
    "wish",
];

/// Words after which "that" is the thing pointed at, or points at the noun
/// after it: prepositions ("after that", "in that file"), conjunctions ("but
/// that's a guess", "if that fails"), question words ("why that happens"), and
/// verbs the cue lexicon does not list ("undo that", "see that error"). An
/// action word the lexicon does list counts as well, when it is a verb.
/// Sorted, for binary search.
const THAT_POINTERS: &[&str] = &[
    "about",
    "above",
    "across",
    "after",
    "against",
    "along",
    "also",
    "although",
    "among",
    "and",
    "around",
    "as",
    "at",
    "because",
    "before",
    "behind",
    "below",
    "beside",
    "besides",
    "between",
    "beyond",
    "but",
    "by",
    "call",
    "copy",
    "despite",
    "do",
    "does",
    "doing",
    "done",
    "during",
    "else",
    "for",
    "from",
    "get",
    "give",
    "handle",
    "how",
    "if",
    "in",
    "inside",
    "into",
    "keep",
    "like",
    "manage",
    "near",
    "need",
    "nor",
    "of",
    "off",
    "on",
    "once",
    "onto",
    "or",
    "otherwise",
    "out",
    "outside",
    "over",
    "past",
    "paste",
    "per",
    "plus",
    "process",
    "put",
    "redo",
    "review",
    "saw",
    "see",
    "since",
    "support",
    "take",
    "than",
    "then",
    "though",
    "through",
    "to",
    "touch",
    "toward",
    "towards",
    "try",
    "under",
    "undo",
    "unless",
    "unlike",
    "until",
    "upon",
    "use",
    "via",
    "want",
    "what",
    "when",
    "whenever",
    "where",
    "whereas",
    "whether",
    "while",
    "who",
    "whom",
    "why",
    "with",
    "within",
    "without",
    "yet",
];

/// Words that open a clause as its subject, so that a "that" before them
/// introduces the clause rather than points: "so that the tests pass", "the
/// bug that you found". Sorted, for binary search.
const SUBJECTS: &[&str] = &[
    "a",
    "all",
    "an",
    "any",
    "anything",
    "both",
    "each",
    "either",
    "every",
    "everyone",
    "everything",
    "he",
    "her",
    "his",
    "i",
    "it",
    "its",
    "my",
    "neither",
    "no",
    "nobody",
    "nothing",
    "our",
    "she",
    "some",
    "someone",
    "something",
    "the",
    "their",
    "there",
    "these",
    "they",
    "this",
    "those",
    "we",
    "you",
    "your",
];

/// Adverbs after which a "that" is the object of the verb before it: "check
/// that again", "test that first". Sorted, for binary search.
const AFTERWORDS: &[&str] = &[
    "again", "also", "anyway", "first", "here", "instead", "later", "now", "out", "please", "too",
    "up",
];

/// Copulas, after which "that" explains ("the problem is that the token
/// expires") unless the copula opens a question ("is that a bug?").
const COPULAS: &[&str] = &[
    "are", "aren't", "be", "been", "is", "isn't", "was", "wasn't", "were", "weren't",
];

/// Question words that open a question around a copula: "what is that error?".
const QUESTION_WORDS: &[&str] = &["how", "what", "when", "where", "which", "who", "why"];

/// `settled` without the action words `text` uses as nouns or as the purpose
/// of another, and with an opening "Return" when nothing else is the action.
///
/// `found` is every cue hit the matcher made, before any prohibition settled
/// one away: "test" in "don't change test files" is still the object of
/// "change". It is returned with the "Return" added, so that grounding reads
/// it as an instruction word. Hits other than action verbs pass through
/// unchanged and in order.
pub(crate) fn strip(
    text: &str,
    tokens: &[Token],
    mut found: Vec<Hit<CueId>>,
    settled: Vec<Hit<CueId>>,
) -> (Vec<Hit<CueId>>, Vec<Hit<CueId>>) {
    let names = antecedents::names(text, tokens);
    let actions = action_ends(&found, &names);
    let breaks = breaks(text);
    let kept: Vec<Hit<CueId>> = settled
        .into_iter()
        .filter(|h| {
            h.id != CueId::ActionVerb
                || !(is_noun(text, h.span, &actions, &names) || clause_verb(text, h.span))
        })
        .collect();
    let mut kept = purposes(text, &breaks, korean(text, &breaks, kept));
    if !kept.iter().any(|h| h.id == CueId::ActionVerb) {
        if let Some(hit) = returned(text, tokens, &found) {
            found.push(hit.clone());
            kept.push(hit);
        } else if let Some(hit) = kept.iter_mut().find(|h| asks_to_return(text, h)) {
            hit.id = CueId::ActionVerb;
        }
    }
    (kept, found)
}

/// `smells` without those `text` uses for something else: a word inside a
/// name or inside a longer cue phrase ("handler" in `src/handler.rs`, "that"
/// in "so that"), a polysemous verb used as a noun ("the type check"), and a
/// "that" that does not point.
///
/// `found` is every cue hit the matcher made, sorted, as it made them.
pub(crate) fn strip_smells(
    text: &str,
    tokens: &[Token],
    found: &[Hit<CueId>],
    smells: Vec<SmellHit>,
) -> Vec<SmellHit> {
    let names = antecedents::names(text, tokens);
    let actions = action_ends(found, &names);
    let verbs: Vec<usize> = found
        .iter()
        .filter(|h| h.id == CueId::ActionVerb && !is_noun(text, h.span, &actions, &names))
        .map(|h| h.span.end)
        .collect();
    smells
        .into_iter()
        .filter(|s| {
            let noun = || is_noun(text, s.span, &actions, &names);
            !antecedents::inside(&names, s.span)
                && !in_phrase(found, s.span)
                && !(s.id == SmellId::PolysemousVerb && !s.term.contains(' ') && noun())
                && !(s.term.eq_ignore_ascii_case("that") && !points(text, s.span, &verbs))
        })
        .collect()
}

/// Where every action word in the text ends that is not part of a name,
/// sorted.
fn action_ends(found: &[Hit<CueId>], names: &[Span]) -> Vec<usize> {
    let mut ends: Vec<usize> = found
        .iter()
        .filter(|h| h.id == CueId::ActionVerb && !antecedents::inside(names, h.span))
        .map(|h| h.span.end)
        .collect();
    ends.sort_unstable();
    ends
}

/// Whether the word at `span` is a noun. `actions` holds where every action
/// word in the text ends, sorted, and `names` the spans of the names in it.
fn is_noun(text: &str, span: Span, actions: &[usize], names: &[Span]) -> bool {
    if antecedents::inside(names, span) {
        return true;
    }
    if !span.slice(text).is_ascii() {
        return false;
    }
    if quoted(text, span) || subject(text, span.end) {
        return true;
    }
    let mut start = span.start;
    if let Some(head) = text[..start].strip_suffix('-') {
        match last_word(head) {
            Some(part) if PREFIXES.binary_search(&fold(part).as_str()).is_ok() => return true,
            Some(part) => {
                // Any other compound is one word, read where it starts: "the
                // re-deploy step" is a noun, and "Re-deploy staging" a verb.
                // One that a hyphen joins to more before it is a longer name,
                // a flag ("--dry-run") or "end-to-end".
                start = head.len() - part.len();
                if text[..start].ends_with('-') {
                    return true;
                }
            }
            // A dash that opens a line is a bullet: "-fix the parser".
            None if head.trim_end_matches([' ', '\t']).is_empty()
                || head.trim_end_matches([' ', '\t']).ends_with('\n') =>
            {
                return false
            }
            // Any other is the name of a flag: "--run", "go test -run".
            None => return true,
        }
    }
    let before = &text[..start];
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
    let folded = fold(word);
    let start = joined.len() - word.len();
    modifies(&folded)
        || PREPOSITIONS.binary_search(&folded.as_str()).is_ok()
        || compound(text, start, &folded)
        || (TOOLS.binary_search(&folded.as_str()).is_ok() && commanded(text, start, actions))
}

/// Whether the Korean after the action word at `span` makes it the verb of a
/// clause that says when or whether something happens, which asks for
/// nothing: push 할 때마다, 리팩터링하면, 업데이트 시, 커밋하는 경우.
fn clause_verb(text: &str, span: Span) -> bool {
    let rest = if span.slice(text).ends_with('하') {
        // 커밋하 holds the 하 the clause is read from.
        &text[span.end - '하'.len_utf8()..]
    } else {
        let rest = text[span.end..].trim_start_matches([' ', '\t']);
        // An object particle may stand between them: 리팩터링을 할 때.
        let rest = rest
            .strip_prefix(['을', '를'])
            .map_or(rest, |r| r.trim_start_matches([' ', '\t']));
        if let Some(after) = rest.strip_prefix('시') {
            // 시 alone is "upon": push 시, 업데이트 시에. Not push 시켜, or 시나리오.
            if after
                .chars()
                .next()
                .is_none_or(|c| !yp_lang::token::is_hangul(c))
                || after.starts_with('에')
                || after.starts_with("마다")
            {
                return true;
            }
        }
        rest
    };
    if KO_WHEN.iter().any(|w| rest.starts_with(w)) {
        return true;
    }
    KO_IF.iter().any(|ending| {
        rest.strip_prefix(ending).is_some_and(|after| {
            // 하면서 is "while doing it", which does not stop asking.
            !after.starts_with(yp_lang::token::is_hangul) && {
                let next = after.trim_start_matches([' ', '\t', ',']);
                !KO_IF_ASKS.iter().any(|asks| next.starts_with(asks))
            }
        })
    })
}

/// Whether the tool name starting at `start` is the object of an action word
/// directly before it: "run npm install".
fn commanded(text: &str, start: usize, actions: &[usize]) -> bool {
    let before = &text[..start];
    let joined = before.trim_end_matches([' ', '\t']);
    joined.len() < before.len() && actions.binary_search(&joined.len()).is_ok()
}

/// Whether `word`, folded, makes the word after it a noun: a determiner, a
/// modifier, or a possessive. "let's" is the one form of "'s" that an
/// instruction follows.
fn modifies(word: &str) -> bool {
    DETERMINERS.binary_search(&word).is_ok()
        || MODIFIERS.binary_search(&word).is_ok()
        || (word.ends_with("'s") && word != "let's")
}

/// Whether `word`, folded and starting at `start`, is the first half of a
/// compound with a determiner before it, which makes the next word its head:
/// "the type check", "an easy fix".
fn compound(text: &str, start: usize, word: &str) -> bool {
    if !word.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || word.ends_with("ly")
        || BREAKS.binary_search(&word).is_ok()
        || FINITE.binary_search(&word).is_ok()
    {
        return false;
    }
    let before = &text[..start];
    let joined = before.trim_end_matches([' ', '\t']);
    if joined.len() == before.len() {
        return false;
    }
    let Some(determiner) = last_word(joined) else {
        return false;
    };
    let folded = fold(determiner);
    if DETERMINERS.binary_search(&folded.as_str()).is_err()
        && !(folded.ends_with("'s") && folded != "let's")
    {
        return false;
    }
    let ahead = &joined[..joined.len() - determiner.len()];
    let head = ahead.trim_end_matches([' ', '\t']);
    !(head.len() < ahead.len()
        && last_word(head).is_some_and(|w| CAUSATIVES.binary_search(&fold(w).as_str()).is_ok()))
}

/// Whether the word ending at `end` is the subject of a verb that follows it
/// on its line: "build is broken", "deploy failed last night".
fn subject(text: &str, end: usize) -> bool {
    let after = &text[end..];
    let next = after.trim_start_matches([' ', '\t']);
    next.len() < after.len() && next_word(next).is_some_and(|w| finite(w, &next[w.len()..]))
}

/// Whether `word`, with `rest` after it on its line, is one of the [`FINITE`]
/// verbs. Not when it is a plural that "of" follows, since none of those
/// verbs takes "of" after it: "Document returns of parse_args", "Investigate
/// causes of the leak". "is of type int" is the copula. Nor "returns" before
/// "in", which names the returns: "Fix returns in parse_args".
fn finite(word: &str, rest: &str) -> bool {
    let folded = fold(word);
    if FINITE.binary_search(&folded.as_str()).is_err() {
        return false;
    }
    let after = rest.trim_start_matches([' ', '\t']);
    if after.len() == rest.len() {
        return true;
    }
    match next_word(after).map(fold).as_deref() {
        Some("of") => {
            !folded.ends_with('s') || matches!(folded.as_str(), "does" | "has" | "is" | "was")
        }
        Some("in") => folded != "returns",
        _ => true,
    }
}

/// Whether `span` stands in quotes on its own, as in `Output: "test"` or the
/// "build" job: a word mentioned rather than used.
fn quoted(text: &str, span: Span) -> bool {
    let open = text[..span.start].chars().next_back();
    let close = text[span.end..].chars().next();
    matches!(
        (open, close),
        (Some('"'), Some('"'))
            | (Some('\''), Some('\''))
            | (Some('“'), Some('”'))
            | (Some('‘'), Some('’'))
    )
}

/// Whether one of the sorted, disjoint `found` cue hits holds `span` and more
/// besides: "that" in "so that", "it" in "while you're at it".
fn in_phrase(found: &[Hit<CueId>], span: Span) -> bool {
    let at = found.partition_point(|h| h.span.start <= span.start);
    at > 0 && {
        let phrase = found[at - 1].span;
        phrase.end >= span.end && phrase.len() > span.len()
    }
}

/// Where the clauses of `text` end: at a line break, or at sentence or clause
/// punctuation followed by a space or by the end. The dot in "src/a.rs" ends
/// nothing.
fn breaks(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let ends = c == '\n'
            || (matches!(c, '.' | ';' | ':' | '!' | '?' | '。' | '？' | '！')
                && chars.peek().is_none_or(|&(_, n)| n.is_whitespace()));
        if ends {
            out.push(i);
        }
    }
    out
}

/// Whether no clause ends between `from` and `to`.
fn same_clause(breaks: &[usize], from: usize, to: usize) -> bool {
    breaks
        .get(breaks.partition_point(|&b| b < from))
        .is_none_or(|&b| b >= to)
}

/// `kept` without the action words in Korean sentences that are another
/// action's object, modifier or purpose.
fn korean(text: &str, breaks: &[usize], kept: Vec<Hit<CueId>>) -> Vec<Hit<CueId>> {
    // Right to left, so that each action word knows whether another follows
    // it in its sentence, and whether that one is Korean.
    let mut keep = vec![true; kept.len()];
    let mut later: Option<(usize, bool)> = None;
    for (i, hit) in kept.iter().enumerate().rev() {
        if hit.id != CueId::ActionVerb {
            continue;
        }
        let headed = later.is_some_and(|(at, _)| same_clause(breaks, hit.span.end, at));
        let object = if hit.term.is_ascii() {
            later.is_some_and(|(_, korean)| korean) && subcommand(text, hit.span)
        } else {
            modifier(text, hit) || purpose(text, hit.span.end)
        };
        if headed && object {
            keep[i] = false;
        } else {
            later = Some((hit.span.start, !hit.term.is_ascii()));
        }
    }
    kept.into_iter()
        .zip(keep)
        .filter_map(|(hit, keep)| keep.then_some(hit))
        .collect()
}

/// Whether the Korean action noun `hit` stands before another word without
/// 하다 or 시키다 to make it the verb: 업데이트 버튼, 마이그레이션을 실행하도록.
fn modifier(text: &str, hit: &Hit<CueId>) -> bool {
    if KO_NOUN_ACTIONS.binary_search(&hit.term.as_str()).is_err() {
        return false;
    }
    let rest = &text[hit.span.end..];
    if KO_JOINED.iter().any(|j| rest.starts_with(j)) {
        return false;
    }
    let rest = rest.strip_prefix(KO_PARTICLES).unwrap_or(rest);
    let next = rest.trim_start_matches([' ', '\t']);
    let next = next
        .strip_prefix("좀")
        .map_or(next, |n| n.trim_start_matches([' ', '\t']));
    next.starts_with(|c: char| yp_lang::token::is_hangul(c) || c.is_ascii_alphanumeric())
        && !KO_VERBALIZERS.iter().any(|v| next.starts_with(v))
        && !KO_JOINS
            .iter()
            .any(|j| next.strip_prefix(j).is_some_and(|r| r.starts_with(' ')))
}

/// Whether the English action word at `span` is a tool's subcommand that no
/// 하다 or 시키다 after it makes the verb: "test" in "cargo test 돌려줘" is
/// what 돌려 runs, as it is in "run cargo test", and in "cargo test 해줘" it
/// is what is asked for.
fn subcommand(text: &str, span: Span) -> bool {
    let before = &text[..span.start];
    let joined = before.trim_end_matches([' ', '\t']);
    let tool = joined.len() < before.len()
        && last_word(joined).is_some_and(|w| TOOLS.binary_search(&fold(w).as_str()).is_ok());
    let rest = text[span.end..].trim_start_matches([' ', '\t']);
    let next = rest
        .strip_prefix(KO_PARTICLES)
        .map_or(rest, |r| r.trim_start_matches([' ', '\t']));
    tool && !KO_VERBALIZERS.iter().any(|v| next.starts_with(v))
}

/// Whether the Korean action word ending at `end` carries -도록, "so that":
/// 실행하도록, 업데이트되도록.
fn purpose(text: &str, end: usize) -> bool {
    let rest = &text[end..];
    ["도록", "하도록", "되도록"]
        .iter()
        .any(|ending| rest.starts_with(ending))
}

/// `kept` without the English action words that are the purpose of the one
/// before them: "update the deploy script to run migrations".
fn purposes(text: &str, breaks: &[usize], kept: Vec<Hit<CueId>>) -> Vec<Hit<CueId>> {
    let mut previous: Option<Span> = None;
    kept.into_iter()
        .filter(|hit| {
            if hit.id != CueId::ActionVerb {
                return true;
            }
            let purpose = hit.term.is_ascii()
                && previous.is_some_and(|p| purpose_of(text, breaks, p, hit.span));
            // Even a purpose moves the start of the next search on, so that each
            // stretch of text between two action words is read once.
            previous = Some(hit.span);
            !purpose
        })
        .collect()
}

/// Whether the action word at `verb` follows "to" and an object of the action
/// word at `previous`, in the same clause.
fn purpose_of(text: &str, breaks: &[usize], previous: Span, verb: Span) -> bool {
    let before = text[..verb.start].trim_end_matches([' ', '\t']);
    if before.len() == verb.start {
        return false;
    }
    let Some(to) = last_word(before) else {
        return false;
    };
    let to_start = before.len() - to.len();
    if !to.eq_ignore_ascii_case("to") || previous.end >= to_start {
        return false;
    }
    let mut words = text[previous.end..to_start]
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '\'' || c == '’'))
        .filter(|w| !w.is_empty())
        .peekable();
    words.peek().is_some()
        && same_clause(breaks, previous.end, to_start)
        && words.all(|w| JOINS.binary_search(&fold(w).as_str()).is_err())
}

/// The first "return" in `tokens` that opens a sentence or a clause and that
/// no cue phrase holds, as an action-verb hit.
fn returned(text: &str, tokens: &[Token], found: &[Hit<CueId>]) -> Option<Hit<CueId>> {
    // Sorted by start, with the furthest end reached so far, so that whether
    // any of them holds a word is one binary search.
    let mut claimed: Vec<Span> = found.iter().map(|h| h.span).collect();
    claimed.sort_unstable();
    let mut furthest = 0;
    let reach: Vec<usize> = claimed
        .iter()
        .map(|c| {
            furthest = furthest.max(c.end);
            furthest
        })
        .collect();
    let held = |span: Span| {
        let at = claimed.partition_point(|c| c.start <= span.start);
        at > 0 && reach[at - 1] >= span.end
    };
    tokens
        .iter()
        .filter(|t| t.kind == TokenKind::Word && t.text == "return")
        .find(|t| !held(t.span) && opens_clause(text, t.span.start) && !subject(text, t.span.end))
        .map(|t| Hit {
            id: CueId::ActionVerb,
            term: t.span.slice(text).to_string(),
            span: t.span,
        })
}

/// Whether the word at `at` opens a sentence or a clause: it starts the text
/// or a line, follows punctuation, or follows a word like "and" or "just" that
/// no modal stands before. "it should also return monomials" says what
/// something should do, as "should return" does.
fn opens_clause(text: &str, at: usize) -> bool {
    let before = text[..at].trim_end_matches([' ', '\t']);
    match before.chars().next_back() {
        None => true,
        Some(c) if c.is_ascii_alphanumeric() => last_word(before).is_some_and(|lead| {
            let ahead = before[..before.len() - lead.len()].trim_end_matches([' ', '\t']);
            RETURN_LEADS.binary_search(&fold(lead).as_str()).is_ok()
                && !last_word(ahead)
                    .is_some_and(|w| FINITE.binary_search(&fold(w).as_str()).is_ok())
        }),
        Some(c) => matches!(
            c,
            '\n' | '.' | '!' | '?' | ';' | ':' | ',' | '(' | '-' | '*' | '>' | '"' | '“'
        ),
    }
}

/// Whether `hit` is 반환 or 리턴 asked for: 반환해줘, 리턴해 주세요,
/// 반환하세요.
fn asks_to_return(text: &str, hit: &Hit<CueId>) -> bool {
    let endings = match hit.term.as_str() {
        "반환해" | "리턴해" => KO_ASKING,
        "반환" | "리턴" => KO_ASKING_HA,
        _ => return false,
    };
    hit.id == CueId::IoSpec
        && endings.iter().any(|ending| {
            text[hit.span.end..]
                .strip_prefix(ending)
                .is_some_and(|rest| !rest.starts_with(yp_lang::token::is_hangul))
        })
}

/// Whether the "that" at `span` points at something, as "this" does, rather
/// than joining a clause to the noun or the verb before it.
///
/// A relative "that" says which thing a noun is: "write a function that
/// returns the sum of a list" scored 486.4 with it charged as a vague
/// pronoun, against 634.2 with "which", and with an index it was marked as
/// pointing at nothing. A complementizer joins a clause: "make sure that",
/// "assume that". Neither points. Of the 837 "that"s in SWE-bench Lite's
/// issues and HumanEvalComm's problems, this reads 758 as one or the other,
/// keeps 72, and leaves 7 to the cue phrases that hold them ("so that"). Read
/// by hand, all but six of the 72 point; the six join a clause, as in "The
/// problem is that, when ..." and "I see that initializer is executed". In a
/// third of the 758, the only ones that point are a few determiners after
/// verbs the lists do not hold ("renders that string").
///
/// "that" points when it ends its clause ("fix that", "what is that?"), opens
/// a sentence ("That breaks login"), follows a preposition, a conjunction or a
/// verb ("after that", "but that's a guess", "undo that change"), or follows
/// a copula that opens a question ("is that a bug?"). It does not when a
/// subject follows it ("so that the tests pass", "the bug that you found"),
/// or when a noun stands before it ("a function that returns", "the change
/// that broke it"). After a word that takes a clause ("make sure", "check"),
/// it points only when a verb or an adverb follows it: "make sure that
/// works", "check that again". A line break inside a sentence is read past,
/// because wrapped text runs a clause across it: "the number of elements
/// that\n need to be changed".
fn points(text: &str, span: Span, verbs: &[usize]) -> bool {
    let next = after(text, span.end);
    if next == Next::End {
        return true;
    }
    let (prev, at) = before(text, span.start);
    if let Prev::Word(word) = &prev {
        if let Some(asks) = asked_with(text, word, at) {
            return asks;
        }
    }
    if next.opens_clause() {
        return false;
    }
    if span.slice(text) == "That" {
        return true;
    }
    match prev {
        Prev::Edge => true,
        Prev::Closer => false,
        Prev::Word(word) => {
            let noun = matches!(before(text, at.start).0, Prev::Word(w) if modifies(&w));
            if noun {
                false
            } else if THAT_HEADS.binary_search(&word.as_str()).is_ok() {
                matches!(&next, Next::Word(w) if FINITE.binary_search(&w.as_str()).is_ok()
                    || AFTERWORDS.binary_search(&w.as_str()).is_ok())
            } else {
                THAT_POINTERS.binary_search(&word.as_str()).is_ok()
                    || verbs.binary_search(&at.end).is_ok()
            }
        }
    }
}

/// For a "that" after the copula `word` at `at`: whether the copula opens a
/// question, which makes the "that" what is asked about. `None` when `word`
/// is not a copula.
fn asked_with(text: &str, word: &str, at: Span) -> Option<bool> {
    if let Some(stem) = word.strip_suffix("'s") {
        return (stem != "let").then(|| QUESTION_WORDS.contains(&stem));
    }
    if !COPULAS.contains(&word) {
        return None;
    }
    Some(match before(text, at.start).0 {
        Prev::Edge => true,
        Prev::Word(w) => QUESTION_WORDS.contains(&w.as_str()),
        Prev::Closer => false,
    })
}

/// What stands before a "that".
#[derive(Debug, PartialEq)]
enum Prev {
    /// The start of the text, clause punctuation, or a line that ends without
    /// a word.
    Edge,
    /// The end of a name or a quote on the same line: `` `numref` that ``,
    /// "next_smallest() that".
    Closer,
    /// A word, folded.
    Word(String),
}

/// What follows a "that".
#[derive(Debug, PartialEq)]
enum Next {
    /// The end of the text or of the clause.
    End,
    /// Code, a quote, a bracket, a number or a name, all of which open a
    /// phrase.
    Opener,
    /// A word, folded. A contraction, "that's", reads as "is".
    Word(String),
}

impl Next {
    fn opens_clause(&self) -> bool {
        match self {
            Next::Opener => true,
            Next::Word(w) => SUBJECTS.binary_search(&w.as_str()).is_ok(),
            Next::End => false,
        }
    }
}

/// What stands before `at`, read past spaces and line breaks, and where.
fn before(text: &str, at: usize) -> (Prev, Span) {
    let head = &text[..at];
    let trimmed = head.trim_end();
    let line = head[trimmed.len()..].contains('\n');
    let Some(last) = trimmed.chars().next_back() else {
        return (Prev::Edge, Span::new(0, 0));
    };
    if last.is_ascii_alphanumeric() || last == '_' {
        let word = last_word_with(trimmed, |c| {
            c.is_ascii_alphanumeric() || matches!(c, '_' | '\'' | '’')
        });
        let start = trimmed.len() - word.len();
        return (Prev::Word(fold(word)), Span::new(start, trimmed.len()));
    }
    let at = Span::new(trimmed.len() - last.len_utf8(), trimmed.len());
    if !line && matches!(last, ')' | ']' | '}' | '`' | '"' | '\'' | '’' | '”') {
        return (Prev::Closer, at);
    }
    (Prev::Edge, at)
}

/// What follows `at`, read past spaces and line breaks.
fn after(text: &str, at: usize) -> Next {
    let rest = text[at..].trim_start();
    let Some(first) = rest.chars().next() else {
        return Next::End;
    };
    if matches!(first, '\'' | '’') {
        let contracted = rest[first.len_utf8()..].starts_with(|c: char| c.is_ascii_alphabetic());
        return if contracted {
            Next::Word("is".into())
        } else {
            Next::Opener
        };
    }
    if matches!(
        first,
        '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '…' | '—' | '–'
    ) {
        return Next::End;
    }
    if !first.is_ascii_alphabetic() {
        return Next::Opener;
    }
    let Some(word) = next_word(rest) else {
        return Next::Opener;
    };
    let tail = &rest[word.len()..];
    let named = word.contains('_')
        || word.chars().skip(1).any(|c| c.is_ascii_uppercase())
        || (tail.starts_with(['.', '/', '\\', '('])
            && tail[1..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == ')'))
        || tail.starts_with("::");
    if named {
        Next::Opener
    } else {
        Next::Word(fold(word))
    }
}

/// Lowercase, with a phone's U+2019 apostrophe read as the ASCII one.
fn fold(word: &str) -> String {
    word.to_ascii_lowercase().replace('’', "'")
}

/// The word `s` starts with, apostrophes included: "doesn't".
fn next_word(s: &str) -> Option<&str> {
    let end = s
        .char_indices()
        .take_while(|&(_, c)| c.is_ascii_alphanumeric() || c == '_' || c == '\'' || c == '’')
        .last()
        .map(|(i, c)| i + c.len_utf8())?;
    Some(&s[..end])
}

/// The word `s` ends with, if it ends with one.
fn last_word(s: &str) -> Option<&str> {
    let word = last_word_with(s, |c| c.is_ascii_alphanumeric() || c == '\'' || c == '’');
    (!word.is_empty()).then_some(word)
}

/// The run of characters `s` ends with that `part` accepts.
fn last_word_with(s: &str, part: impl Fn(char) -> bool) -> &str {
    let start = s
        .char_indices()
        .rev()
        .take_while(|&(_, c)| part(c))
        .last()
        .map_or(s.len(), |(i, _)| i);
    &s[start..]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verbs(text: &str) -> Vec<String> {
        let found = yp_lang::resources().unwrap().cue_matcher.find(text);
        let settled = crate::negation::settle(text, &found);
        strip(text, &yp_lang::tokenize(text), found, settled)
            .0
            .into_iter()
            .filter(|h| h.id == CueId::ActionVerb)
            .map(|h| h.term)
            .collect()
    }

    fn smells(text: &str) -> Vec<String> {
        let r = yp_lang::resources().unwrap();
        let found = r.cue_matcher.find(text);
        strip_smells(text, &yp_lang::tokenize(text), &found, r.smells.find(text))
            .into_iter()
            .map(|h| h.term)
            .collect()
    }

    #[test]
    fn the_word_lists_are_sorted_so_the_search_works() {
        for list in [
            DETERMINERS,
            MODIFIERS,
            FINITE,
            BREAKS,
            CAUSATIVES,
            JOINS,
            TOOLS,
            PREPOSITIONS,
            PREFIXES,
            KO_NOUN_ACTIONS,
            RETURN_LEADS,
            THAT_HEADS,
            THAT_POINTERS,
            SUBJECTS,
            AFTERWORDS,
        ] {
            let mut sorted = list.to_vec();
            sorted.sort_unstable();
            assert_eq!(list, sorted.as_slice());
        }
    }

    #[test]
    fn every_korean_action_noun_is_an_action_the_lexicon_lists() {
        let cues = yp_lang::resources().unwrap();
        let actions: Vec<&str> = cues
            .cues
            .table
            .terms
            .iter()
            .zip(&cues.cues.table.categories)
            .filter(|(_, id)| **id == CueId::ActionVerb)
            .map(|(t, _)| t.as_str())
            .collect();
        for noun in KO_NOUN_ACTIONS {
            assert!(actions.contains(noun), "{noun} is not an action verb");
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
    fn an_action_word_before_a_verb_is_its_subject() {
        // Each of these scored as high as "fix the bug" or higher, with the
        // subject as its objective.
        for text in [
            "build is broken",
            "deploy failed last night",
            "test fails on main",
            "lint doesn't pass on CI",
            "When DEBUG is True, the page leaks settings",
        ] {
            assert!(verbs(text).is_empty(), "{text}: {:?}", verbs(text));
        }
        assert_eq!(verbs("docker build fails, fix the Dockerfile"), ["fix"]);
        // A plural an instruction takes as its object is not a verb.
        assert_eq!(verbs("fix crashes on startup"), ["fix"]);
        assert_eq!(verbs("log errors to stderr"), ["log"]);
        assert_eq!(verbs("fix broken tests"), ["fix"]);
        assert_eq!(verbs("test is_valid on empty input"), ["test"]);
    }

    #[test]
    fn the_second_word_after_a_determiner_is_the_head_of_a_compound() {
        assert_eq!(verbs("fix the login test"), ["fix"]);
        assert_eq!(verbs("rename the CI build to nightly"), ["rename"]);
        assert_eq!(
            verbs("I can attach the proposed fix here"),
            Vec::<String>::new()
        );
        // Not across a word that modifies nothing, nor after a causative.
        assert_eq!(verbs("the user can test it"), ["test"]);
        assert_eq!(verbs("let the user test it"), ["test"]);
        assert_eq!(verbs("make the parser log errors"), ["make", "log"]);
    }

    #[test]
    fn a_subcommand_another_action_runs_names_the_command() {
        assert_eq!(verbs("run npm install and fix the errors"), ["run", "fix"]);
        assert_eq!(verbs("run cargo test and fix the failures"), ["run", "fix"]);
        assert_eq!(verbs("run pip install -r requirements.txt"), ["run"]);
        // Typed on its own, the command is what is asked for.
        assert_eq!(verbs("cargo test"), ["test"]);
        assert_eq!(verbs("go fix the parser"), ["fix"]);
        // In Korean the verb that runs it comes after it, and 하다 makes the
        // command itself the verb.
        assert_eq!(verbs("cargo test 돌려줘"), ["돌려"]);
        assert_eq!(
            verbs("cargo test 랑 cargo clippy 돌리는 워크플로 만들어줘"),
            ["만들어"]
        );
        assert_eq!(verbs("cargo test 해줘"), ["test"]);
        assert_eq!(verbs("cargo test 해주고 실패하면 고쳐줘"), ["test", "고쳐"]);
    }

    #[test]
    fn an_action_word_a_prefix_or_a_flag_holds_is_part_of_a_noun() {
        // The first three counted the joined word as a second objective once
        // commit, push and install were actions.
        for (text, wanted) in [
            (
                "Add a pre-commit hook that runs prettier on staged files.",
                vec!["Add"],
            ),
            ("Add a pre-push hook that runs the tests.", vec!["Add"]),
            ("Fix the post-install script in package.json.", vec!["Fix"]),
            ("Only change non-test files.", vec!["change"]),
            ("Fix the re-deploy step in scripts/deploy.sh.", vec!["Fix"]),
            ("Implement soft-delete for users.", vec!["Implement"]),
            ("Add a dry-run flag to scripts/deploy.sh.", vec!["Add"]),
            ("Run scripts/deploy.sh with --dry-run", vec!["Run"]),
            ("go test -run TestVerify ./auth", vec!["test"]),
        ] {
            assert_eq!(verbs(text), wanted, "{text}");
        }
        // A compound that opens an instruction is its verb, and a dash that
        // opens a line is a bullet.
        for (text, wanted) in [
            ("Re-deploy staging after the fix.", vec!["deploy"]),
            ("Re-enable the flaky test in tests/auth.rs.", vec!["enable"]),
            ("Re-run the failing tests.", vec!["Re-run"]),
            ("Cross-compile the CLI for aarch64.", vec!["compile"]),
            ("-fix the parser\n-add a test", vec!["fix", "add"]),
        ] {
            assert_eq!(verbs(text), wanted, "{text}");
        }
    }

    #[test]
    fn an_action_word_after_a_preposition_is_its_object() {
        // "Trigger" is not an action word, and "push" was the objective.
        assert!(verbs("Trigger the deploy on push to main.").is_empty());
        for (text, wanted) in [
            ("Run the tests on push to main.", vec!["Run"]),
            ("Run the tests on every push to main.", vec!["Run"]),
            ("Run clippy upon commit.", vec!["Run"]),
            ("Run the linter before commit.", vec!["Run"]),
            ("Lint the code during build.", vec!["Lint"]),
            ("Fix the race: use SELECT ... FOR UPDATE.", vec!["Fix"]),
            // "to" and a verb is no preposition and its object.
            ("I want to fix the parser", vec!["fix"]),
        ] {
            assert_eq!(verbs(text), wanted, "{text}");
        }
    }

    #[test]
    fn a_korean_clause_of_time_or_condition_asks_for_nothing() {
        // push was read as a third objective, beside the "test" that
        // 돌리는 runs.
        assert_eq!(
            verbs("push 할 때마다 cargo test 랑 cargo clippy 돌리는 GitHub Actions 워크플로 만들어줘."),
            ["만들어"]
        );
        for (text, wanted) in [
            ("push 하면 테스트 돌려줘.", vec!["돌려"]),
            (
                "main 에 merge 될 때 배포 스크립트를 수정해줘.",
                vec!["수정해"],
            ),
            ("push 시 테스트를 돌려줘.", vec!["돌려"]),
            ("리팩터링을 하기 전에 테스트 추가해줘.", vec!["추가해"]),
            ("커밋하는 경우 테스트를 돌려줘.", vec!["돌려"]),
            ("리팩터링할 때 테스트도 같이 고쳐줘.", vec!["고쳐"]),
            // A request after the condition asks for what it names, and so
            // does "while doing it".
            ("리팩터링하면 좋겠어.", vec!["리팩터링"]),
            ("main 에 push 하면 돼.", vec!["push"]),
            ("리팩터링하면서 테스트도 고쳐줘.", vec!["리팩터링", "고쳐"]),
            ("push 시켜줘.", vec!["push"]),
        ] {
            assert_eq!(verbs(text), wanted, "{text}");
        }
        // Forbidden, it asks for nothing either.
        assert!(verbs("main 에 push 하면 안 돼.").is_empty());
    }

    #[test]
    fn a_plural_before_of_and_returns_before_in_are_nouns() {
        // The first two lost their objective (424.2 to 270.3): "returns" was
        // read as the verb whose subject the word before it was.
        for (text, wanted) in [
            ("Document returns of parse_args.", vec!["Document"]),
            ("Fix returns in parse_args.", vec!["Fix"]),
            (
                "Investigate causes of the memory leak in worker.py.",
                vec!["Investigate"],
            ),
            ("Log runs of the nightly job to stderr.", vec!["Log"]),
        ] {
            assert_eq!(verbs(text), wanted, "{text}");
        }
        // The verbs still make the word before them their subject.
        for text in [
            "build is broken",
            "build is of no use",
            "test returns early",
            "Test runs twice, fix the fixture.",
        ] {
            assert!(
                !verbs(text)
                    .iter()
                    .any(|v| v.eq_ignore_ascii_case("test") || v == "build"),
                "{text}"
            );
        }
    }

    #[test]
    fn a_word_inside_a_name_is_part_of_the_name() {
        assert_eq!(verbs("fix the panic in src/test/foo.rs"), ["fix"]);
        assert_eq!(verbs("fix the panic in scripts/deploy.sh"), ["fix"]);
        assert_eq!(verbs("build() is broken, fix it"), ["fix"]);
        assert_eq!(verbs("self.build() hangs, fix it"), ["fix"]);
        assert!(verbs("test() is flaky").is_empty());
        assert_eq!(
            verbs("RepoIndex::build 를 from_files 로 분리해줘"),
            ["분리해"]
        );
    }

    #[test]
    fn to_and_an_action_after_an_object_is_its_purpose() {
        for (text, wanted) in [
            ("update the deploy script to run migrations", vec!["update"]),
            ("update scripts/deploy.sh to run migrations", vec!["update"]),
            ("change the log level to debug", vec!["change"]),
            ("explain how to deploy it", vec!["explain"]),
            ("add a flag to disable the cache", vec!["add"]),
            // Nothing before it to be the purpose of.
            ("I want to fix the parser", vec!["fix"]),
            // A new clause, or a request joined to the first.
            (
                "fix the parser and try to refactor the lexer",
                vec!["fix", "refactor"],
            ),
            (
                "fix the parser. Try to refactor it",
                vec!["fix", "refactor"],
            ),
        ] {
            assert_eq!(verbs(text), wanted, "{text}");
        }
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
        let settled = crate::negation::settle(text, &found);
        let (settled, _) = strip(text, &yp_lang::tokenize(text), found, settled);
        assert!(settled
            .iter()
            .any(|h| h.id == CueId::ScopeConstraint && h.term == "refactor"));
        assert_eq!(verbs(text), ["fix"]);
        // Not under a prohibition, what follows "no" is a noun.
        assert_eq!(verbs("add a test with no retry"), ["add"]);
    }

    #[test]
    fn a_korean_action_noun_with_another_action_after_it_is_its_object() {
        // These read the noun as a second objective. The test this replaces
        // said Korean was left alone, and asserted that "마이그레이션 파일
        // 추가해줘" asked for two things.
        assert_eq!(verbs("마이그레이션 파일 추가해줘"), ["추가해"]);
        assert_eq!(verbs("업데이트 버튼 고쳐줘"), ["고쳐"]);
        assert_eq!(verbs("리팩터링 브랜치 머지해줘"), ["머지해"]);
        assert_eq!(
            verbs("마이그레이션을 실행하도록 배포 스크립트를 수정해줘"),
            ["수정해"]
        );
        // 하다 makes it the verb, and alone it keeps its reading.
        assert_eq!(verbs("의존성 업데이트해줘"), ["업데이트해"]);
        assert_eq!(verbs("리팩터링 해줘"), ["리팩터링"]);
        assert_eq!(verbs("리팩터링을 좀 해줘"), ["리팩터링"]);
        assert_eq!(verbs("리팩터링 부탁해"), ["리팩터링"]);
        assert_eq!(
            verbs("리팩터링하고 테스트 추가해줘"),
            ["리팩터링", "추가해"]
        );
        assert_eq!(verbs("업데이트와 배포 둘 다 해줘"), ["업데이트"]);
        assert_eq!(verbs("테스트 추가해줘"), ["추가해"]);
    }

    #[test]
    fn a_korean_verb_made_with_hada_is_an_action() {
        for (text, wanted) in [
            ("파서 테스트해줘", "테스트해"),
            ("커밋해줘", "커밋해"),
            ("main 에 푸시해줘", "푸시해"),
            ("이 브랜치 머지해줘", "머지해"),
            ("두 브랜치를 병합해줘", "병합해"),
            ("빌드해줘", "빌드해"),
            ("PR 리뷰해줘", "리뷰해"),
            ("의존성 설치해줘", "설치해"),
            ("롤백해줘", "롤백해"),
        ] {
            assert_eq!(verbs(text), [wanted], "{text}");
        }
        assert_eq!(verbs("커밋하고 푸시해줘"), ["커밋하", "푸시해"]);
        // The nouns stay nouns.
        assert_eq!(verbs("커밋 메시지 고쳐줘"), ["고쳐"]);
        assert_eq!(verbs("빌드 스크립트 고쳐줘"), ["고쳐"]);
    }

    #[test]
    fn an_opening_return_asks_for_what_it_returns() {
        assert_eq!(
            verbs("Return Err(AuthError::Expired) from verify_token instead of panicking."),
            ["Return"]
        );
        assert_eq!(
            verbs("If the input is empty, return an empty list."),
            ["return"]
        );
        assert_eq!(verbs("Err(AuthError::Expired) 를 반환해줘"), ["반환해"]);
        assert_eq!(verbs("빈 리스트를 반환하세요"), ["반환"]);
        // Another action is the objective, and "return" does not add one.
        assert_eq!(
            verbs("Make verify_token return Err(AuthError::Expired)"),
            ["Make"]
        );
        // Forbidden, described or named, it asks for nothing.
        for text in [
            "Don't return None",
            "It should return Err",
            "it should also return monomials such as x1*x2",
            "the return value is wrong",
            "Return values of non converged clustering",
            "an early return skips the cleanup",
            "Err 를 반환해야 해",
        ] {
            assert!(verbs(text).is_empty(), "{text}: {:?}", verbs(text));
        }
    }

    #[test]
    fn an_opening_return_is_not_also_a_result_shape() {
        let text = "반환해줘";
        let found = yp_lang::resources().unwrap().cue_matcher.find(text);
        let settled = crate::negation::settle(text, &found);
        let (settled, _) = strip(text, &yp_lang::tokenize(text), found, settled);
        assert!(
            !settled.iter().any(|h| h.id == CueId::IoSpec),
            "{settled:?}"
        );
    }

    #[test]
    fn hits_other_than_action_verbs_pass_through_in_order() {
        let text = "only in src/cli.rs, refactor parse_args so it returns Config; tests must pass";
        let found = yp_lang::resources().unwrap().cue_matcher.find(text);
        let (settled, _) = strip(text, &yp_lang::tokenize(text), found.clone(), found.clone());
        assert_eq!(settled, found);
    }

    fn component(s: &crate::Score, id: &str) -> f64 {
        [&s.actionability, &s.clarity, &s.context]
            .iter()
            .flat_map(|a| &a.components)
            .find(|c| c.id == id)
            .unwrap()
            .earned
    }

    #[test]
    fn a_relative_that_costs_nothing_in_either_mode() {
        // 486.4 with "that", 634.2 with "which"; with an index the "that" was
        // marked as pointing at nothing.
        let that = "write a function that returns the sum of a list";
        let which = "write a function which returns the sum of a list";
        assert_eq!(
            crate::score(that).unwrap().total,
            crate::score(which).unwrap().total
        );
        let corpus = crate::MapCorpus::new(10, &[("verify_token", 1, 3, 1)]);
        for text in [
            that,
            "add a test that checks verify_token rejects expired tokens",
        ] {
            let s = crate::score_with(text, Some(&corpus)).unwrap();
            let deixis = s
                .grounding
                .as_ref()
                .unwrap()
                .components
                .iter()
                .find(|c| c.id == "deixis");
            assert_eq!(deixis.unwrap().detail, "no unattached references", "{text}");
            assert!(
                !s.marks.iter().any(|m| m.kind.starts_with("pronoun")),
                "{text}"
            );
        }
        // "so that" is charged once, as the cue it is.
        let text = "In src/auth/login.rs, change verify_token so that it returns \
                    Err(AuthError::Expired) instead of panicking on an expired token.";
        let s = crate::score(text).unwrap();
        assert!(component(&s, "acceptance") > 0.0);
        assert_eq!(s.clarity.components[0].detail, "no ambiguity smells");
    }

    #[test]
    fn a_name_an_action_word_heads_is_not_a_second_objective_in_either_mode() {
        let corpus = crate::MapCorpus::new(10, &[("build", 1, 2, 1)]);
        for corpus in [None, Some(&corpus as &dyn crate::Corpus)] {
            let s = crate::score_with("build() is broken, fix it", corpus).unwrap();
            assert_eq!(
                component(&s, "singularity"),
                crate::params::actionability::SINGULARITY_MAX
            );
        }
    }

    #[test]
    fn a_terse_report_asks_for_nothing_and_a_compound_is_not_a_vague_verb() {
        let fix = crate::score("fix the bug").unwrap().total;
        for text in [
            "build is broken",
            "deploy failed last night",
            "test() is flaky",
        ] {
            let s = crate::score(text).unwrap();
            assert_eq!(component(&s, "objective"), 0.0, "{text}");
            assert!(s.total < fix, "{text}: {} against {fix}", s.total);
        }
        let s = crate::score("fix the type check errors in src/auth/login.rs").unwrap();
        assert!(
            !s.clarity.components[0].detail.contains("polysemous"),
            "{}",
            s.clarity.components[0].detail
        );
        let s = crate::score("update the deploy script to run migrations").unwrap();
        assert_eq!(
            component(&s, "singularity"),
            crate::params::actionability::SINGULARITY_MAX
        );
    }

    #[test]
    fn a_hada_verb_asks_as_its_english_twin_does() {
        for (korean, english) in [
            ("파서 테스트해줘", "test the parser"),
            ("커밋해줘", "commit it"),
        ] {
            for text in [korean, english] {
                let s = crate::score(text).unwrap();
                assert_eq!(
                    component(&s, "objective"),
                    crate::params::actionability::OBJECTIVE_MAX,
                    "{text}"
                );
            }
        }
        let s = crate::score("업데이트 버튼 고쳐줘").unwrap();
        assert_eq!(
            component(&s, "singularity"),
            crate::params::actionability::SINGULARITY_MAX
        );
    }

    #[test]
    fn an_opening_return_scores_as_the_make_form_does() {
        // 180.1 with no objective, against 564.7 for "Make verify_token
        // return ...".
        let returned =
            crate::score("Return Err(AuthError::Expired) from verify_token instead of panicking.")
                .unwrap();
        let made =
            crate::score("Make verify_token return Err(AuthError::Expired) instead of panicking.")
                .unwrap();
        assert_eq!(
            component(&returned, "objective"),
            crate::params::actionability::OBJECTIVE_MAX
        );
        assert!(
            returned.total >= made.total - 1.0,
            "{} vs {}",
            returned.total,
            made.total
        );
        assert_eq!(
            component(&made, "singularity"),
            crate::params::actionability::SINGULARITY_MAX
        );
    }

    #[test]
    fn many_purposes_and_modifiers_cost_about_what_as_many_requests_do() {
        // In each pair the same action words are found, but in the first
        // text every one of them looks back to the same earlier action, or
        // ahead to the same later one, across all the others. Read that way
        // for each of them, the work would be quadratic. Timed against each
        // other, the two texts measure how the work grows, not how fast the
        // machine is.
        use std::time::{Duration, Instant};
        let time = |text: &str| {
            let r = yp_lang::resources().unwrap();
            let tokens = yp_lang::tokenize(text);
            let found = r.cue_matcher.find(text);
            let settled = crate::negation::settle(text, &found);
            let smells = r.smells.find(text);
            (0..3)
                .map(|_| {
                    let (found, settled, smells) = (found.clone(), settled.clone(), smells.clone());
                    let start = Instant::now();
                    std::hint::black_box(strip_smells(text, &tokens, &found, smells));
                    std::hint::black_box(strip(text, &tokens, found, settled));
                    start.elapsed()
                })
                .min()
                .unwrap()
        };
        for (heavy, light) in [
            (
                format!("fix a{}", " to run".repeat(10_000)),
                format!("fix a{}", ". run".repeat(10_000)),
            ),
            (
                format!("{}고쳐줘", "업데이트 ".repeat(10_000)),
                format!("{}고쳐줘", "업데이트. ".repeat(10_000)),
            ),
            (
                "the type check that returns ".repeat(4_000),
                "the type. check. that. returns. ".repeat(4_000),
            ),
        ] {
            let (heavy_time, light_time) = (time(&heavy), time(&light));
            assert!(
                heavy_time < light_time * 20 + Duration::from_millis(100),
                "{:?}...: {heavy_time:?} against {light_time:?}",
                &heavy[..12]
            );
        }
    }

    #[test]
    fn a_relative_or_a_complementizer_that_does_not_point() {
        for text in [
            "write a function that returns the sum of a list",
            "add a test that checks verify_token rejects expired tokens",
            "remove the code that logs passwords",
            "make sure that the tests pass",
            "assume that x is positive",
            "the bug that you found in `login()` that panics",
            "change verify_token so that it returns Err",
            "the number of elements that\n    need to be changed",
            "a road that's a perfectly straight line",
            "a test that fails on expired tokens",
            "is it really that slow?",
        ] {
            assert!(
                !smells(text).iter().any(|s| s.eq_ignore_ascii_case("that")),
                "{text}: {:?}",
                smells(text)
            );
        }
    }

    #[test]
    fn a_demonstrative_that_points() {
        for text in [
            "fix that",
            "what is that?",
            "That breaks login, fix it",
            "fix it, that breaks login",
            "but that's just a guess",
            "if that fails, roll back",
            "after login, undo that change",
            "is that a bug?",
            "make sure that works",
            "check that again",
            "fix that bug",
            "convert that to binary",
        ] {
            assert!(
                smells(text).iter().any(|s| s.eq_ignore_ascii_case("that")),
                "{text}: {:?}",
                smells(text)
            );
        }
    }

    #[test]
    fn a_word_a_name_or_a_cue_phrase_holds_is_not_a_smell() {
        assert!(smells("fix src/handler.rs").is_empty());
        assert!(smells("fix the bug in src/config.rs").is_empty());
        assert!(smells("fix this.state in App.tsx").is_empty());
        assert!(smells("handle() hangs on an empty list").is_empty());
        assert_eq!(smells("change verify_token so that it returns Err"), ["it"]);
        // Outside a name the same words are charged.
        assert_eq!(smells("fix the handler"), ["handler"]);
    }

    #[test]
    fn a_polysemous_verb_used_as_a_noun_is_not_charged() {
        for text in [
            "fix the type check errors in src/auth/login.rs",
            "the health check fails on staging",
            "update the update script",
            "address the review comments on PR 482",
            "add a null check to verify_token",
        ] {
            let found = smells(text);
            assert!(
                found.iter().filter(|s| *s != "update").count() == 0
                    && found.iter().filter(|s| *s == "update").count() <= 1,
                "{text}: {found:?}"
            );
        }
        // As verbs they still are.
        assert_eq!(smells("check the logs"), ["check"]);
        assert_eq!(smells("make the parser handle empty input"), ["handle"]);
        assert_eq!(smells("update the docs"), ["update"]);
    }
}
