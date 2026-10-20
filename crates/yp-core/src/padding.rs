//! A cue word that names nothing is padding.
//!
//! The actionability and context axes count distinct cue words: a result
//! shape, a finish line, a boundary. Each component saturates fast, so three
//! or four bare words came close to its maximum, and clarity's absence credit
//! opened in full with any one cue and eight tokens, which the cue words
//! supplied themselves. "Refactor `x`. Returns json, input schema. Tests
//! pass, assert, verify. Only, keep, unchanged." scored 909.8, an S, and a
//! 48-word list of cue words 995.4. With the demo's sample loaded, the same
//! eleven words after "Refactor verify_token." took it from 503.1 to 912.1,
//! above the 860.3 of the prompt that names the file, the function and the
//! test: an index checks names, not cue words.
//!
//! So a result-shape, finish-line or boundary cue counts only when its clause
//! is about something:
//!
//! - **A name.** A word that is not a cue, a function word, a single letter
//!   or a value, such as a path, an identifier, code or a noun, vouches for
//!   one cue in its clause, and for one only in the whole prompt: a name
//!   repeated in every clause backs one of them. The clause may then complete
//!   the frame around it with one more cue, "Keep `foo` unchanged", "Don't
//!   change the response". The verb a prohibition forbids is a name.
//! - **A value.** A number, a number word or a quoted letter is what a cue
//!   that takes a value takes, "returns 404", "exit code 1", 404 를 반환해,
//!   and vouches for that cue alone; see [`VALUE_CUES`].
//! - **A part of the result.** An I/O noun, the response, the schema, JSON,
//!   입력, can be what a boundary or a check is about. In a clause that
//!   grammar holds together it vouches for one acceptance or scope cue and
//!   counts itself: "Keep the response schema unchanged", 입력과 출력은
//!   그대로 유지하고. So it does for a boundary when the two stand alone,
//!   "Return type unchanged", and a format beside the I/O cue it describes
//!   counts with it, "JSON output", JSON 반환; see [`FORMATS`].
//! - **A statement about the result.** An I/O cue counts in a sentence, or as
//!   the value of a label such as "Output:"; see [`names_its_own`]. A word
//!   alone between commas after such a sentence is the next item of its
//!   list: "the input, output, and schema".
//! - **A statement about the whole task.** A clause whose one cue states a
//!   whole criterion counts every time, "No new warnings.", "경고 없이"; see
//!   [`STATEMENTS`]. And once in each category, a phrase of cue words with
//!   nothing named counts, "Make sure all tests pass", "Keep everything else
//!   unchanged", 나머지는 그대로 유지해, when it holds no more than
//!   [`GENERIC_CUES`] of the category's cue words besides a label,
//!   "Acceptance criteria:", or a topic, 나머지는.
//!
//! None of this holds for cue words in a list: a clause with more than
//! [`LIST_CUES`] of them beyond its names, or a run of short clauses, each a
//! cue with at most one more word beside it and any letters and values, two
//! of them a single cue word or three of them keywords, as in "Tests pass,
//! assert, verify." and "Returns 1, input 2, schema 3." A sentence is a
//! clause of more than one word held together by a function word other than
//! an article, by a Korean particle, or by the objective and the article of
//! its object; see [`ARTICLES`].
//!
//! The cue words left over are padding. They earn nothing, and count for
//! nothing toward the substance of clarity's absence credit, which counts the
//! cue words that stay only as far as other words back them; see
//! [`unbacked`].
//!
//! What counts as a clause: text between commas, semicolons, full stops,
//! exclamation and question marks that end a phrase, list items, and line
//! breaks that start one. A label ending in a colon reads on into what
//! follows, so "Done means: the test suite passes" is one clause, and so is
//! "Expected output:" over a code block. A line wrapped in the middle of a
//! sentence, as pasted docstrings and issues are, stays one.
//!
//! Measured through the WebAssembly build, as the demo page calls it: the
//! thirteen-word list above and its Korean twin both score 321.8 without an
//! index, the 48-word one 500.5, and the eleven words after "Refactor
//! verify_token." take it from 503.1 to 529.9 with the sample. With a
//! different number after each cue word they took it to 915.3, an S, and
//! take it to 537.1. None of the showcase's 32 prompts moves, with or
//! without the sample, nor any of the 32 in the Korean fixture or the 300
//! SWE-bench Lite issues. Of 1,803 honest prompts gathered from every source
//! at hand, three move: two HumanEvalComm variants made incomplete by ending
//! on a dangling "then", which now earns nothing, and the boilerplate "Add
//! tests. Expected output must equal actual output. Only in the test folder;
//! keep everything else unchanged.", from 817.0 to 791.0.
//!
//! Cue words written into sentences still count, because the rules read
//! words and not claims: "Refactor verify_token so that it returns json and
//! takes the input schema, and make sure the tests pass, and assert and
//! verify, and only keep it unchanged." scores 836.5 with the sample.

use std::collections::{HashMap, HashSet};

use yp_lang::token::code_regions;
use yp_lang::{CueId, Hit, Span, Token, TokenKind};

use crate::stats::PromptStats;

/// The most cue words of one category a generic statement holds, and the
/// most its label holds: a marker and what it marks, "make sure" with "all
/// tests pass", 그대로 with 유지해. "Add a config loader. Acceptance
/// criteria: all tests pass." holds two in its label and one after it, and
/// counted together they made it a list: 316.1, against 552.5.
const GENERIC_CUES: usize = 2;

/// The most cue words a clause holds beyond its names and still reads as a
/// statement rather than a list: two generic statements joined, as in "모든
/// 테스트가 통과해야 하고 다른 건 건드리지 마", hold four. One particle made
/// a longer run a sentence, and "리팩터링해줘 모든 테스트가 건드리지 마 다른
/// 건 반환 `x` 리턴 테스트 통과" scored 866.1 with all six of its cue words
/// counted. As a list it scores 320.0.
const LIST_CUES: usize = 2 * GENERIC_CUES;

/// Articles open a noun phrase and hold no clause together, so they make no
/// sentence of the cue words they open. Counted as glue, one each turned the
/// thirteen-word list into "Refactor the x. Returns the json, the input
/// schema. The tests pass, the assert, the verify. Only the, keep the,
/// unchanged the.", which scored 696.0; it scores 337.6. After "Refactor
/// verify_token." it took the sample to 797.6, and takes it to 533.3.
const ARTICLES: &[&str] = &["a", "an", "the"];

/// I/O cue words that are nouns for a part of the result, which an
/// acceptance or scope cue in their clause can be about. Without them, "Make
/// the endpoint return 404 instead of 500 when the user does not exist. Keep
/// the response schema unchanged." lost its boundary: 679.5, against 768.8.
/// Sorted, for binary search.
const IO_NOUNS: &[&str] = &[
    "argument",
    "arguments",
    "endpoint",
    "exit code",
    "input",
    "inputs",
    "json",
    "output",
    "outputs",
    "parameter",
    "parameters",
    "payload",
    "request body",
    "response",
    "return type",
    "return types",
    "return value",
    "return values",
    "route",
    "schema",
    "signature",
    "status code",
    "stderr",
    "stdout",
    "yaml",
    "라우트",
    "리턴값",
    "매개변수",
    "반환값",
    "상태 코드",
    "스키마",
    "시그니처",
    "엔드포인트",
    "요청",
    "응답",
    "인자",
    "입력",
    "종료 코드",
    "출력",
    "파라미터",
    "표준 출력",
];

/// I/O cue words that name a format or a stream. Beside the I/O cue it
/// describes, a format says what the result looks like: "JSON output",
/// "Returns JSON", "YAML input", JSON 반환. Sorted, for binary search.
const FORMATS: &[&str] = &["json", "stderr", "stdout", "yaml", "표준 출력"];

/// Cue words that take a value: "returns 404", "exit code 1", "expected 5",
/// "limit to 100", 404 반환. A number beside one of them is what it returns,
/// expects or allows. Sorted, for binary search.
const VALUE_CUES: &[&str] = &[
    "->",
    "=>",
    "accepts",
    "coverage",
    "exit code",
    "expect",
    "expected",
    "expects",
    "fails with",
    "for input",
    "given",
    "input",
    "inputs",
    "limit to",
    "limited to",
    "must equal",
    "must return",
    "output",
    "outputs",
    "produces",
    "response",
    "restricted to",
    "return value",
    "return values",
    "returns",
    "should equal",
    "should return",
    "status code",
    "takes",
    "yields",
    "결과는",
    "결과로",
    "기대값",
    "돌려주",
    "돌려줘야",
    "리턴",
    "리턴값",
    "리턴해",
    "반환",
    "반환값",
    "반환해",
    "상태 코드",
    "예상 결과",
    "예상값",
    "응답",
    "입력",
    "종료 코드",
    "출력",
    "커버리지",
];

/// Numbers written as words. "one" is a function word already, and 열 names a
/// column as often as ten. Sorted, for binary search.
const NUMBER_WORDS: &[&str] = &[
    "billion",
    "eight",
    "eighteen",
    "eighty",
    "eleven",
    "fifteen",
    "fifty",
    "five",
    "forty",
    "four",
    "fourteen",
    "hundred",
    "million",
    "nine",
    "nineteen",
    "ninety",
    "seven",
    "seventeen",
    "seventy",
    "six",
    "sixteen",
    "sixty",
    "ten",
    "thirteen",
    "thirty",
    "thousand",
    "three",
    "twelve",
    "twenty",
    "two",
    "zero",
    "넷",
    "다섯",
    "둘",
    "셋",
    "아홉",
    "여덟",
    "여섯",
    "일곱",
    "하나",
];

/// Cue words that state a whole criterion, a check or a boundary with its
/// subject: "tests pass", "no new warnings", "nothing else", 테스트 통과,
/// 건드리지 마. Standing alone, each says something every time. Most other
/// cues of several words only mark one, "make sure", "must pass", 완료 조건,
/// or name a kind of test, 회귀 테스트: counted as statements, "Refactor `x`.
/// Make sure. Must pass. Should pass. Has to pass. Only in. Do not. Must
/// not." scored 562.2, and it scores 328.4. Sorted, for binary search.
const STATEMENTS: &[&str] = &[
    "backward compatible",
    "backwards compatible",
    "build passes",
    "ci passes",
    "lint passes",
    "no new dependencies",
    "no new deps",
    "no new warnings",
    "no regression",
    "no warnings",
    "nothing else",
    "pass all tests",
    "pass the tests",
    "passes all tests",
    "passes the tests",
    "suite pass",
    "suite passes",
    "test passes",
    "test suite pass",
    "test suite passes",
    "tests pass",
    "tests passing",
    "without errors",
    "without warnings",
    "zero warnings",
    "건드리지 마",
    "건드리지 말",
    "건드리지마",
    "건드리지말",
    "경고 없어야",
    "경고 없이",
    "경고가 없어야",
    "그대로 둬",
    "깨지지 않",
    "바꾸지 마",
    "바꾸지 말",
    "바꾸지마",
    "바꾸지말",
    "반드시 통과",
    "손대지 마",
    "손대지 말",
    "손대지마",
    "손대지말",
    "수정하지 마",
    "수정하지 말",
    "전부 통과",
    "지우지 마",
    "지우지 말",
    "지우지마",
    "지우지말",
    "추가하지 마",
    "추가하지 말",
    "테스트 통과",
    "테스트가 통과",
    "테스트를 통과",
    "하위 호환",
];

/// The categories a generic statement can speak for. That all the tests
/// pass, or that nothing else changes, is said of the whole task.
const GENERIC: [CueId; 2] = [CueId::Acceptance, CueId::ScopeConstraint];

/// Whether a cue of this category says what it is about on its own. Most I/O
/// cue words are nouns for a part of the result, the response, the output,
/// JSON, the schema, so a sentence of them says something with nothing named
/// beside them: made to wait for a name, "Change the endpoint so the response
/// is JSON." fell from 674.9 to 322.6. Acceptance and scope cue words mark
/// something else, "verify" or "only", and need a name.
fn names_its_own(id: CueId) -> bool {
    id == CueId::IoSpec
}

/// Words that hold a sentence together without naming anything it is about.
/// Sorted, for binary search.
///
/// Determiners, pronouns, prepositions, conjunctions, auxiliaries and the
/// adverbs that only grade or order: none of them tells a cue what it is
/// about. "Only the, keep the" names no more than "Only, keep" does.
/// "thing", "things" and "stuff" are here for the same reason, although the
/// smell lexicon charges them as well.
const EN_FUNCTION_WORDS: &[&str] = &[
    "about",
    "above",
    "across",
    "after",
    "again",
    "against",
    "all",
    "along",
    "already",
    "also",
    "although",
    "always",
    "am",
    "among",
    "an",
    "and",
    "another",
    "any",
    "anybody",
    "anyone",
    "anything",
    "are",
    "around",
    "as",
    "at",
    "be",
    "because",
    "been",
    "before",
    "being",
    "below",
    "between",
    "beyond",
    "both",
    "but",
    "by",
    "can",
    "could",
    "did",
    "do",
    "does",
    "doing",
    "done",
    "down",
    "during",
    "each",
    "either",
    "else",
    "even",
    "ever",
    "every",
    "everybody",
    "everyone",
    "everything",
    "few",
    "for",
    "from",
    "had",
    "has",
    "have",
    "having",
    "he",
    "her",
    "here",
    "hers",
    "him",
    "his",
    "how",
    "however",
    "i",
    "if",
    "in",
    "into",
    "is",
    "it",
    "its",
    "itself",
    "just",
    "least",
    "less",
    "let",
    "like",
    "many",
    "may",
    "maybe",
    "me",
    "might",
    "mine",
    "more",
    "most",
    "much",
    "must",
    "my",
    "need",
    "needs",
    "neither",
    "nobody",
    "none",
    "nor",
    "not",
    "nothing",
    "now",
    "of",
    "off",
    "often",
    "on",
    "once",
    "one",
    "ones",
    "onto",
    "or",
    "other",
    "others",
    "ought",
    "our",
    "ours",
    "out",
    "over",
    "own",
    "per",
    "quite",
    "rather",
    "really",
    "same",
    "several",
    "shall",
    "she",
    "should",
    "so",
    "some",
    "somebody",
    "someone",
    "something",
    "sometimes",
    "still",
    "stuff",
    "such",
    "than",
    "that",
    "the",
    "their",
    "theirs",
    "them",
    "themselves",
    "then",
    "there",
    "these",
    "they",
    "thing",
    "things",
    "this",
    "those",
    "though",
    "through",
    "to",
    "too",
    "toward",
    "towards",
    "under",
    "unless",
    "until",
    "up",
    "upon",
    "us",
    "very",
    "via",
    "was",
    "we",
    "were",
    "what",
    "whatever",
    "when",
    "where",
    "whether",
    "which",
    "while",
    "who",
    "whom",
    "whose",
    "why",
    "will",
    "with",
    "within",
    "would",
    "yet",
    "you",
    "your",
    "yours",
    "yourself",
];

/// The Korean counterparts that stand as words of their own: bound nouns,
/// the auxiliary and light verbs, pronouns, adverbs of degree, time and
/// insistence, and the particles written apart after a name, as in
/// "tests/auth.rs 는". Sorted, for binary search.
const KO_FUNCTION_WORDS: &[&str] = &[
    "가",
    "거",
    "건",
    "걸",
    "것",
    "것도",
    "것은",
    "것을",
    "것이",
    "게",
    "경우",
    "계속",
    "과",
    "그",
    "그거",
    "그냥",
    "까지",
    "꼭",
    "나",
    "너",
    "는",
    "다",
    "다른",
    "대신",
    "더",
    "도",
    "돼",
    "되게",
    "되도록",
    "된",
    "될",
    "됨",
    "등",
    "때",
    "또",
    "로",
    "를",
    "마",
    "만",
    "말",
    "말고",
    "모두",
    "모든",
    "못",
    "및",
    "반드시",
    "부터",
    "수",
    "아무",
    "아무것도",
    "안",
    "없게",
    "없는",
    "없어",
    "에",
    "에서",
    "와",
    "우리",
    "으로",
    "은",
    "을",
    "의",
    "이",
    "이거",
    "있게",
    "있는",
    "있어",
    "잘",
    "저",
    "저거",
    "전부",
    "절대",
    "정도",
    "좀",
    "주세요",
    "줘",
    "처럼",
    "하게",
    "하고",
    "하는",
    "하면",
    "하지",
    "할",
    "함",
    "항상",
    "해",
    "해서",
    "해야",
    "해요",
    "해주세요",
    "해줘",
];

/// The particles Korean writes onto a noun, by their last syllable: 버그를,
/// 출력은, 로직에서.
const KO_PARTICLES: &str = "은는이가을를의에서도만과와로";

/// What a token contributes to telling whether a cue is about something.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Punctuation, a filler or a list marker: not counted by anything.
    Nothing,
    /// A single letter standing in for a name or labelling a list item: `x`,
    /// the "b" of a list labelled a, b, c.
    Letter,
    /// A word that holds the clause together without naming anything.
    Function,
    /// The action the prompt asks for.
    Objective,
    /// A cue word, wholly inside its cue. `particle` when Korean writes a
    /// particle onto it, which makes its clause a phrase rather than a list
    /// entry, as a function word does in English. `io` when the cue is an I/O
    /// cue.
    Cue { particle: bool, io: bool },
    /// A word that names something the cues in its clause can be about.
    /// `bare` when it is a plain word, rather than a name written as code or
    /// a verb that a prohibition forbids.
    Content { bare: bool },
    /// A number, a number word or a quoted letter: what a cue that takes a
    /// value takes, "returns 404", 404 반환, and a name of nothing else.
    Value,
}

/// The cue categories whose words must be about something.
fn is_bound(id: CueId) -> bool {
    matches!(
        id,
        CueId::IoSpec | CueId::Acceptance | CueId::ScopeConstraint
    )
}

/// The categories whose words are cue words rather than names. An example
/// marker need not be about anything in its own clause: "For example," is
/// followed by its example, and whether anything concrete backs it is
/// already asked.
fn is_evidence(id: CueId) -> bool {
    is_bound(id) || id == CueId::ExampleMarker
}

/// `settled` without the padding, and `stats` with how many of its cue
/// words nothing backs.
///
/// `found` is every cue hit the matcher made, before prohibitions were
/// settled: a forbidden verb that became a scope cue still names the action
/// it forbids, as "change" in "don't change the schema" does. The hits that
/// are kept pass through unchanged and in order.
pub(crate) fn strip(
    text: &str,
    tokens: &[Token],
    found: &[Hit<CueId>],
    settled: Vec<Hit<CueId>>,
    stats: PromptStats,
) -> (Vec<Hit<CueId>>, PromptStats) {
    let verdict = judge(text, tokens, found, &settled);
    let stats = stats.with_unbacked(unbacked(tokens, &verdict));
    if !verdict.hits.contains(&true) {
        return (settled, stats);
    }
    let kept = settled
        .into_iter()
        .zip(verdict.hits)
        .filter(|(_, padding)| !padding)
        .map(|(h, _)| h)
        .collect();
    (kept, stats)
}

/// How many of the typed tokens count for nothing toward clarity's
/// substance: padding, and the cue words and single letters standing in for
/// names that nothing backs.
///
/// Clarity rewards the absence of vague words, and none of these can be
/// vague. A cue word that is padding is about nothing and counts for nothing:
/// counted as other cue words are, the eleven words after "Refactor
/// verify_token." took it to 592.4 with the sample, and they take it to
/// 529.9. An article that opens padding counts for nothing either, and so do
/// the function words of a clause of nothing but padding: "Refactor `x`. Only
/// all, keep all, unchanged all, assert all, verify all." scored 560.4 with
/// its five "all"s backing five cue words, and scores 320.0. So does the
/// number of a list item of nothing but padding: counted, eight such numbers
/// held a numbered list of cue words at 644.1, and it scores 355.6. And so
/// does the word or value beside each cue of a list: counted, the numbers in
/// "Refactor verify_token. Returns 1, input 2, schema 3. Tests pass 4, ..."
/// held it at 724.6 with the sample, and it scores 537.1.
///
/// A cue word that stays counts only as far as the words that could have
/// been back it: each name, the objective, each I/O word that stays and each
/// function word backs one cue phrase, however many words the phrase has.
/// One word each, "Fix login. Tests pass, no warnings." would have lost 96
/// points to its own finish line. A Korean particle is the article or
/// preposition English writes apart, so 출력은 backs as "the output" does;
/// without it "출력은 그대로 유지해." lost 48.1 that "Keep the output
/// unchanged." keeps.
fn unbacked(tokens: &[Token], verdict: &Verdict) -> usize {
    let (mut backing, mut phrases, mut words, mut idle) = (0usize, 0usize, 0usize, 0usize);
    let mut last: Option<usize> = None;
    for (i, (token, role)) in tokens.iter().zip(&verdict.roles).enumerate() {
        match role {
            // Only what the other measures count as content can back it.
            _ if token.kind == TokenKind::Punct => {}
            Role::Objective => backing += 1,
            // The word beside a cue in a clause of nothing but padding, the
            // way the number of such a list item is.
            Role::Content { .. } | Role::Value if verdict.idle[i] => idle += 1,
            Role::Content { .. } | Role::Value => backing += 1 + usize::from(has_particle(token)),
            // An I/O cue word that stays names a part of the result.
            Role::Cue { io: true, particle } if !verdict.covered[i] => {
                backing += 1 + usize::from(*particle)
            }
            Role::Function if verdict.glue[i] => idle += 1,
            Role::Function => backing += 1,
            Role::Cue { .. } if verdict.covered[i] => idle += 1,
            Role::Cue { .. } => {
                words += 1;
                if verdict.phrase[i] != last {
                    phrases += 1;
                    last = verdict.phrase[i];
                }
            }
            Role::Letter => {
                words += 1;
                phrases += 1;
            }
            // The number of an item of nothing but padding.
            Role::Nothing if verdict.idle[i] && token.kind == TokenKind::Number => idle += 1,
            Role::Nothing => {}
        }
    }
    if phrases == 0 {
        return idle;
    }
    idle + (words * phrases.saturating_sub(backing) + phrases / 2) / phrases
}

/// Which hits are padding, parallel to the hits judged, and what each token
/// is, parallel to the tokens.
struct Verdict {
    hits: Vec<bool>,
    roles: Vec<Role>,
    /// Whether each token lies in a padding hit.
    covered: Vec<bool>,
    /// For each cue word, the hit it stands in, so that a phrase of several
    /// words counts once.
    phrase: Vec<Option<usize>>,
    /// Function words that hold nothing together; see [`glue`].
    glue: Vec<bool>,
    /// Whether each token lies in a clause of nothing but padding.
    idle: Vec<bool>,
}

/// One clause, as far as its cues are concerned.
#[derive(Default)]
struct Clause {
    /// The names in it, in order, each once.
    names: Vec<String>,
    /// How many of them have been used up, by cues earlier in the clause or
    /// by the same name earlier in the prompt.
    used: usize,
    /// Its I/O cue words that are nouns, in order, each once; see
    /// [`IO_NOUNS`].
    io_nouns: Vec<String>,
    /// Its distinct I/O, acceptance and scope cue words, and their
    /// categories, a bit each.
    cues: HashSet<String>,
    kinds: u8,
    /// How many example markers it holds.
    markers: usize,
    /// How many words it holds, values aside, and how many of them are cue
    /// words, articles and other function words.
    words: usize,
    cue_words: usize,
    articles: usize,
    function_words: usize,
    /// Whether a function word other than an article, or a particle, holds
    /// it together.
    function: bool,
    /// Whether it holds the objective.
    objective: bool,
    /// Whether a cue in it is written as its topic; see [`topic`].
    topic: bool,
    /// Whether it opens with a label that introduces an example or a value:
    /// "Input:", "Expected:", 출력:.
    labelled: bool,
    /// Whether it holds a cue that states a whole criterion; see
    /// [`STATEMENTS`].
    states: bool,
    /// Whether it holds a word of a heading, which no padding can be.
    headed: bool,
    /// Whether it holds a name that is more than a plain word: a path, an
    /// identifier, code, a verb a prohibition forbids.
    solid: bool,
    /// How many single letters and how many values it holds, which name
    /// nothing; see [`Role::Value`].
    letters: usize,
    values: usize,
    /// Whether one of its names vouched for one of its cues.
    vouched: bool,
    /// Whether the cue that completes the frame around that name is taken.
    framed: bool,
}

impl Clause {
    /// Whether it holds a name or the objective, which no padding can be.
    fn says(&self) -> bool {
        self.objective || self.headed || !self.names.is_empty()
    }

    /// How many things it holds that say something: cues and plain names.
    /// Letters and values name nothing.
    fn units(&self) -> usize {
        self.cues.len() + self.markers + self.names.len()
    }

    /// Whether it is short enough to be an entry of a list: a cue and at
    /// most one more word, a plain one or another cue, beside letters and
    /// values, with nothing to hold them together but an article. "returns
    /// 1", "verify red", "JSON output", "the assert".
    fn short(&self) -> bool {
        !self.objective
            && !self.function
            && !self.solid
            && !self.labelled
            && self.cues.len() + self.markers >= 1
            && self.units() <= 2
    }

    /// Whether it can stand in a list of cue words: one cue alone, or a
    /// short clause that is not a criterion stated whole with the cue that
    /// marks it. Read as an entry, "Make sure the tests pass." lost its finish
    /// line beside the list after it, and "Fix login. Make sure the tests
    /// pass. Verify it. Assert it." scored 223.4, against 412.8.
    fn entry(&self) -> bool {
        self.alone() || (self.short() && !self.criterion())
    }

    /// Whether it is a criterion stated whole and the cue that marks it,
    /// both of one category: "Make sure the tests pass".
    fn criterion(&self) -> bool {
        self.cues.len() + self.markers == GENERIC_CUES
            && self.states
            && self.kinds.count_ones() == 1
    }

    /// Whether it holds nothing that could end a list: no cue, no name and no
    /// objective, only punctuation, letters, values or function words, as
    /// between "e.g." and a line break. Read as the end of one, that blank
    /// split the entries of the 48-word list, which scored 758.4 for it and
    /// scores 500.5.
    fn blank(&self) -> bool {
        !self.says() && self.cues.is_empty() && self.markers == 0
    }

    /// Whether it is a single cue word with nothing beside it but articles
    /// and function words: "assert", "the verify", "Only all".
    fn single(&self) -> bool {
        self.alone() && self.cue_words == 1 && self.letters + self.values == 0
    }

    /// Whether it is an entry that reads as a keyword: a single cue word, or
    /// a cue and one more thing, "verify 7", "only red", "JSON output". A
    /// cue of several words alone states a criterion, "tests pass".
    fn keyword(&self) -> bool {
        self.entry() && (self.single() || self.units() == 2 || self.letters + self.values > 0)
    }

    /// Whether it is a list of cue words, whatever holds it together: more
    /// than [`LIST_CUES`] of them beyond the names it holds.
    fn list(&self) -> bool {
        self.cues.len() > self.names.len() + LIST_CUES
    }

    /// Whether it reads as a sentence: more than one word, held together by
    /// a function word other than an article or by a particle, or by the
    /// objective and the article of its object, "Update the schema", and not
    /// a list.
    fn sentence(&self) -> bool {
        (self.function || (self.objective && self.articles > 0)) && self.words > 1 && !self.list()
    }

    /// Whether an I/O cue in it says what the result looks like with nothing
    /// named beside it: a sentence, or the value of a label such as
    /// "Output:".
    fn states_io(&self) -> bool {
        self.sentence() || (self.labelled && !self.list())
    }

    /// Whether an I/O noun in it can be what a cue of category `id` is about:
    /// grammar holds the two together, as in "Keep the response schema
    /// unchanged" and 입력과 출력은 그대로 유지하고, or the noun and a
    /// boundary are all there is, as in "Return type unchanged". Read as
    /// padding, that took "Fix the expired-token panic in verify_token.
    /// Return type unchanged." from 678.9 to 468.5.
    fn holds_nouns(&self, id: CueId) -> bool {
        (self.function
            || self.articles > 0
            || self.labelled
            || (id == CueId::ScopeConstraint && self.short()))
            && self.words > 1
            && !self.list()
    }

    /// Whether it is one cue and nothing else, but articles, function words
    /// and values: "assert", "the verify", "Only all", "returns 1 and 2".
    fn alone(&self) -> bool {
        self.cues.len() + self.markers == 1
            && self.words - self.articles - self.function_words == self.cue_words
    }

    /// Whether one category's cue words, none of them vouched for, make it a
    /// statement about the whole task: `label` of them in a label before
    /// what it labels, "Acceptance criteria:" or 나머지는, and `statement` in
    /// the rest. A sentence can, a clause that opens with its topic can,
    /// 나머지는 그대로 유지해, and so can a phrase of no more than
    /// [`GENERIC_CUES`] cues of one category around a criterion stated
    /// whole, "Keep backwards compatible". Example markers count among them:
    /// "Refactor `x`. Must pass tests pass for example e.g." scored 464.9
    /// with its second sentence read as one statement, and scores 314.7. Read
    /// as a phrase, "tests pass only" took "Refactor must pass `fn` 1
    /// returns. tests pass only" to 653.4, where it scores 484.8.
    fn generic(&self, label: usize, statement: usize) -> bool {
        let phrase = self.sentence()
            || (self.topic && !self.list())
            || (self.cues.len() + self.markers <= GENERIC_CUES
                && self.states
                && self.kinds.count_ones() == 1);
        phrase && label <= GENERIC_CUES && statement <= GENERIC_CUES
    }
}

fn judge(text: &str, tokens: &[Token], found: &[Hit<CueId>], settled: &[Hit<CueId>]) -> Verdict {
    // Settled hits in text order. Questions are appended after the cue hits,
    // so the list as given is not sorted.
    let mut order: Vec<usize> = (0..settled.len()).collect();
    order.sort_by_key(|&i| (settled[i].span.start, settled[i].span.end));
    let starts: Vec<usize> = order.iter().map(|&i| settled[i].span.start).collect();
    let mut verbs: Vec<Span> = found
        .iter()
        .filter(|h| h.id == CueId::ActionVerb)
        .map(|h| h.span)
        .collect();
    verbs.sort_unstable();

    let items = crate::stats::list_items(text);
    let roles: Vec<Role> = tokens
        .iter()
        .map(|t| {
            let item = items.partition_point(|item| item.end < t.span.start);
            match items.get(item) {
                // "1." numbers the item and names nothing. Read as a name it
                // vouched for the cue words after it, and a numbered list of
                // them scored 708.0; it scores 355.6.
                Some(item) if item.marker <= t.span.start && t.span.end <= item.start => {
                    Role::Nothing
                }
                // "- a", "- 나": the label of a list item, not a word in it.
                Some(item) if item.start == t.span.start && crate::stats::is_label(t) => {
                    Role::Letter
                }
                _ => role(text, t, settled, &order, &starts, &verbs),
            }
        })
        .collect();
    let phrase: Vec<Option<usize>> = tokens
        .iter()
        .zip(&roles)
        .map(|(t, role)| {
            // The last hit starting before the token ends is the only one
            // that can overlap it.
            let upto = starts.partition_point(|&s| s < t.span.end);
            order[..upto]
                .last()
                .copied()
                .filter(|&i| matches!(role, Role::Cue { .. }) && settled[i].span.end > t.span.start)
        })
        .collect();
    if !settled.iter().any(|h| is_bound(h.id)) {
        let covered = vec![false; tokens.len()];
        let glue = glue(tokens, &roles, &covered, &covered);
        return Verdict {
            hits: vec![false; settled.len()],
            roles,
            idle: covered.clone(),
            covered,
            phrase,
            glue,
        };
    }

    let spans = clauses(text);
    let clause_of = |at: usize| spans.partition_point(|c| c.end <= at).min(spans.len() - 1);
    let code = code_regions(text);
    let labels: Vec<usize> = spans.iter().map(|s| label_end(text, &code, *s)).collect();
    // A label of one plain word is a heading, "Scope:" or "Requirements:",
    // and names nothing the cues after it are about: read as a name, "Scope"
    // took "Refactor `x`.\nScope:\n- only" to 450.4, where it scores 355.9.
    let mut heading = vec![0usize; spans.len()];
    for t in tokens.iter().filter(|t| t.kind != TokenKind::Punct) {
        let c = clause_of(t.span.start);
        if t.span.end <= labels[c] {
            heading[c] += 1;
        }
    }
    // A value is what the cue beside it takes, when that cue takes one: the
    // word after the cue, or the one before it, past any function words,
    // "returns 404", "f(2) => 4", 404 를 반환해. A cue takes one value, and a
    // value names nothing else, so "verify 7" says no more than "verify".
    // Read as names, a different number after each cue word took "Refactor
    // verify_token." to 915.3 with the sample, and two of them to 922.7.
    let word = |j: usize| !matches!(roles[j], Role::Nothing | Role::Function);
    let mut next = vec![tokens.len(); tokens.len() + 1];
    for j in (0..tokens.len()).rev() {
        next[j] = if word(j) { j } else { next[j + 1] };
    }
    let mut prev: Vec<Option<usize>> = vec![None; tokens.len() + 1];
    for j in 0..tokens.len() {
        prev[j + 1] = if word(j) { Some(j) } else { prev[j] };
    }
    let mut takes: Vec<Option<String>> = vec![None; settled.len()];
    let mut taken = vec![false; tokens.len()];
    for &h in &order {
        let hit = &settled[h];
        if !is_bound(hit.id)
            || VALUE_CUES
                .binary_search(&hit.term.to_lowercase().trim())
                .is_err()
        {
            continue;
        }
        let c = clause_of(hit.span.start);
        let after = next[tokens.partition_point(|t| t.span.start < hit.span.end)];
        let before = prev[tokens.partition_point(|t| t.span.end <= hit.span.start)];
        let value = [Some(after), before].into_iter().flatten().find(|&j| {
            j < tokens.len()
                && roles[j] == Role::Value
                && !taken[j]
                && clause_of(tokens[j].span.start) == c
        });
        if let Some(j) = value {
            taken[j] = true;
            takes[h] = Some(name(&tokens[j]));
        }
    }

    let mut clauses: Vec<Clause> = spans.iter().map(|_| Clause::default()).collect();
    let mut seen: HashSet<(usize, String)> = HashSet::new();
    for (token, &role) in tokens.iter().zip(&roles) {
        let c = clause_of(token.span.start);
        let clause = &mut clauses[c];
        if !matches!(role, Role::Nothing | Role::Value) {
            clause.words += 1;
        }
        match role {
            Role::Nothing => {}
            Role::Value => clause.values += 1,
            Role::Letter => clause.letters += 1,
            Role::Cue { particle, .. } => {
                clause.cue_words += 1;
                clause.function |= particle;
            }
            Role::Function if is_article(token) => clause.articles += 1,
            Role::Function => {
                clause.function = true;
                clause.function_words += 1;
            }
            Role::Objective => clause.objective = true,
            Role::Content { bare } => {
                if token.span.end <= labels[c] && heading[c] == 1 && token.kind == TokenKind::Word {
                    clause.headed = true;
                    continue;
                }
                clause.solid |= !bare;
                let name = name(token);
                if seen.insert((c, name.clone())) {
                    clause.names.push(name);
                }
            }
        }
    }
    for &i in &order {
        let hit = &settled[i];
        let c = clause_of(hit.span.start);
        let clause = &mut clauses[c];
        if hit.id == CueId::ExampleMarker {
            clause.markers += 1;
            if hit.term.ends_with(':') && hit.span.end <= labels[c] {
                clause.labelled = true;
            }
        }
        if !is_bound(hit.id) {
            continue;
        }
        // 나머지는 그대로 유지해: a topic, and what is said of it. The topic
        // makes its clause a phrase and counts as its label; without either,
        // "디버그 출력을 지워줘. 나머지는 그대로 유지해." lost its boundary,
        // 382.8 against 608.1.
        clause.topic |= topic(text, hit);
        let term = hit.term.to_lowercase();
        clause.states |= STATEMENTS.binary_search(&term.trim()).is_ok();
        if hit.id == CueId::IoSpec
            && IO_NOUNS.binary_search(&term.trim()).is_ok()
            && !clause.io_nouns.contains(&term)
        {
            clause.io_nouns.push(term.clone());
        }
        clause.cues.insert(term);
        clause.kinds |= match hit.id {
            CueId::IoSpec => 1,
            CueId::Acceptance => 2,
            _ => 4,
        };
    }

    // A run of entries is a list of cue words when two of them are a single
    // cue word alone, "Tests pass, assert, verify.", or three of them are
    // keywords, "Returns 1, input 2, schema 3." Its entries state nothing,
    // and the word beside each cue names nothing either: read as a name
    // beside a cue, a plain word after each one took "Refactor
    // verify_token." to 825.1 with the sample. Two keywords make no list,
    // as one or two terse requirements after the task are what they say:
    // read as one, "Add a `yp export` command. JSON output. Return type
    // unchanged. Tests pass." would score 414.4, not 829.0. And only an
    // entry that brings a cue word new to the run counts toward it, as a
    // list of cue words collects them: "Add login tests.\n- verify login\n-
    // verify logout\n- verify refresh" is a checklist, and as a list it
    // scored 251.9, against 422.6.
    let mut listed = vec![false; clauses.len()];
    let mut run = 0usize;
    while run < clauses.len() {
        let end = (run..clauses.len())
            .find(|&c| !clauses[c].entry() && !clauses[c].blank())
            .unwrap_or(clauses.len());
        let (mut singles, mut keywords) = (0usize, 0usize);
        let mut terms: HashSet<&str> = HashSet::new();
        for clause in clauses[run..end].iter().filter(|c| c.keyword()) {
            let new = clause
                .cues
                .iter()
                .fold(clause.cues.is_empty(), |new, t| terms.insert(t) || new);
            if new {
                keywords += 1;
                singles += usize::from(clause.single());
            }
        }
        if singles >= 2 || keywords >= 3 {
            listed[run..end].fill(true);
            for clause in &mut clauses[run..end] {
                clause.names.clear();
                clause.headed = false;
            }
        }
        run = end + 1;
    }

    // A format says what the result looks like beside the I/O cue it
    // describes, "JSON output", "Returns JSON", JSON 반환, and the two count
    // together. Neither names the other, and "Add a `yp export` command.
    // JSON output. Tests pass." scored 569.2 with both as padding, against
    // 763.6.
    let mut counted: HashSet<(CueId, String)> = HashSet::new();
    for (c, clause) in clauses.iter().enumerate() {
        if !listed[c]
            && clause.short()
            && clause.kinds == 1
            && clause.cues.len() == 2
            && clause
                .cues
                .iter()
                .filter(|t| FORMATS.binary_search(&t.trim()).is_ok())
                .count()
                == 1
        {
            counted.extend(clause.cues.iter().map(|t| (CueId::IoSpec, t.clone())));
        }
    }

    // A value vouches for the cue that takes it, and each value does so once.
    // The clause may frame one more cue around the two: without it, "When
    // the token is expired, the endpoint should return 401." lost its
    // "endpoint", 510.8 against 524.5.
    let mut vouched: HashSet<String> = HashSet::new();
    for &i in &order {
        let c = clause_of(settled[i].span.start);
        if !listed[c]
            && !counted.contains(&key(&settled[i]))
            && takes[i].as_ref().is_some_and(|v| vouched.insert(v.clone()))
        {
            counted.insert(key(&settled[i]));
            clauses[c].vouched = true;
        }
    }

    // Each name vouches for one cue in its clause, in text order, and so may
    // a part of the result for a boundary or a check. The clause may then
    // complete the frame around it with one more: "Keep `foo` unchanged",
    // "Don't change the response". Without the frame the second scored
    // 204.2, against 368.1.
    let mut nouns: HashSet<String> = HashSet::new();
    let mut left: Vec<usize> = Vec::new();
    for &i in &order {
        let hit = &settled[i];
        // A cue already counted needs nothing more to count again.
        if !is_bound(hit.id) || counted.contains(&key(hit)) {
            continue;
        }
        let c = clause_of(hit.span.start);
        if listed[c] {
            continue;
        }
        let clause = &mut clauses[c];
        let mut backed = false;
        while clause.used < clause.names.len() && !backed {
            backed = vouched.insert(clause.names[clause.used].clone());
            clause.used += 1;
        }
        if !backed && hit.id != CueId::IoSpec && clause.holds_nouns(hit.id) {
            if let Some(noun) = clause.io_nouns.iter().find(|n| !nouns.contains(*n)) {
                counted.insert((CueId::IoSpec, noun.clone()));
                nouns.insert(noun.clone());
                backed = true;
            }
        }
        if backed {
            clause.vouched = true;
            counted.insert(key(hit));
        } else if clause.vouched && !clause.framed {
            clause.framed = true;
            counted.insert(key(hit));
        } else {
            left.push(i);
        }
    }
    let left: Vec<usize> = left
        .into_iter()
        .filter(|&i| !counted.contains(&key(&settled[i])))
        .collect();

    // An I/O cue says what the result looks like in a sentence, and so does
    // a word alone between commas after one: "the input, output, and
    // schema" is a list the sentence opened, and "output" alone cost
    // "Document the input, output, and schema of each endpoint." 6.2 points.
    let mut lists = vec![false; clauses.len()];
    for &i in &order {
        let c = clause_of(settled[i].span.start);
        if settled[i].id == CueId::IoSpec
            && !listed[c]
            && counted.contains(&key(&settled[i]))
            && clauses[c].states_io()
        {
            lists[c] = true;
        }
    }
    for &i in left.iter().filter(|&&i| names_its_own(settled[i].id)) {
        let c = clause_of(settled[i].span.start);
        let item = c > 0
            && lists[c - 1]
            && clauses[c].cues.len() == 1
            && clauses[c].words == 1
            && spans[c - 1].slice(text).trim_end().ends_with(',');
        if clauses[c].states_io() || item {
            counted.insert(key(&settled[i]));
            lists[c] = true;
        }
    }

    for id in GENERIC {
        let mut first: Vec<usize> = Vec::new();
        // Each clause's cue words of the category, and whether each stands in
        // a label.
        let mut terms: HashMap<usize, HashMap<String, bool>> = HashMap::new();
        for &i in left.iter().filter(|&&i| settled[i].id == id) {
            let hit = &settled[i];
            let c = clause_of(hit.span.start);
            let set = terms.entry(c).or_default();
            if set.is_empty() {
                first.push(c);
            }
            let label = hit.span.end <= labels[c] || topic(text, hit);
            set.entry(hit.term.to_lowercase()).or_insert(label);
        }
        // A clause whose one cue states a whole criterion counts however
        // many there are: "No new warnings.", "경고 없이". As a second
        // statement of its category, "no warnings" in "Fix the import cycle
        // between api.py and models.py. Tests pass, no warnings." would have
        // counted for nothing, 633.9 against 659.9.
        for &c in &first {
            let set = &terms[&c];
            if clauses[c].cues.len() == 1
                && set.len() == 1
                && set
                    .keys()
                    .all(|t| STATEMENTS.binary_search(&t.trim()).is_ok())
            {
                counted.extend(set.keys().map(|t| (id, t.clone())));
            }
        }
        // Once in each category, a generic statement.
        let chosen = first.into_iter().find(|&c| {
            let set = &terms[&c];
            let label = set.values().filter(|&&l| l).count();
            clauses[c].generic(label, set.len() - label)
                && !set.keys().all(|t| counted.contains(&(id, t.clone())))
        });
        if let Some(c) = chosen {
            counted.extend(terms.remove(&c).into_iter().flatten().map(|(t, _)| (id, t)));
        }
    }

    let mut hits: Vec<bool> = settled
        .iter()
        .map(|h| is_bound(h.id) && !counted.contains(&key(h)))
        .collect();

    // A clause of nothing but padding: it has cues, every one of them is
    // padding, and it names nothing and asks for nothing. Its example markers
    // promise nothing either.
    let mut empty: Vec<Option<bool>> = vec![None; clauses.len()];
    for &i in &order {
        if is_bound(settled[i].id) {
            let c = clause_of(settled[i].span.start);
            let all = empty[c].unwrap_or(true);
            empty[c] = Some(all && hits[i] && !clauses[c].says());
        }
    }
    for &i in &order {
        if settled[i].id == CueId::ExampleMarker
            && empty[clause_of(settled[i].span.start)] == Some(true)
        {
            hits[i] = true;
        }
    }

    let padding: Vec<Span> = order
        .iter()
        .filter(|&&i| hits[i])
        .map(|&i| settled[i].span)
        .collect();
    let covered: Vec<bool> = tokens
        .iter()
        .zip(&roles)
        .map(|(t, role)| {
            // The last padding hit starting before the token ends is the
            // only one that can cover it.
            let upto = padding.partition_point(|s| s.start < t.span.end);
            matches!(role, Role::Cue { .. })
                && padding[..upto].last().is_some_and(|s| s.end > t.span.start)
        })
        .collect();
    let idle: Vec<bool> = tokens
        .iter()
        .map(|t| empty[clause_of(t.span.start)] == Some(true))
        .collect();
    let glue = glue(tokens, &roles, &covered, &idle);
    Verdict {
        hits,
        roles,
        covered,
        phrase,
        glue,
        idle,
    }
}

/// The function words that hold nothing together: those of a clause of
/// nothing but padding (`idle`), and articles that open padding or open
/// nothing before the clause ends, "the assert", "keep the,". An article
/// belongs to the phrase it opens.
fn glue(tokens: &[Token], roles: &[Role], covered: &[bool], idle: &[bool]) -> Vec<bool> {
    (0..tokens.len())
        .map(|i| {
            if roles[i] != Role::Function {
                return false;
            }
            if idle[i] {
                return true;
            }
            if !is_article(&tokens[i]) {
                return false;
            }
            let next = (i + 1..tokens.len())
                .find(|&j| tokens[j].kind != TokenKind::Punct || is_break(&tokens[j]));
            match next {
                Some(j) => tokens[j].kind == TokenKind::Punct || covered[j],
                None => true,
            }
        })
        .collect()
}

fn is_article(token: &Token) -> bool {
    token.kind == TokenKind::Word && ARTICLES.contains(&token.text.as_str())
}

/// Punctuation that ends what an article could open.
fn is_break(token: &Token) -> bool {
    token.kind == TokenKind::Punct && token.text.chars().any(|c| ",.;:!?".contains(c))
}

/// Whether a Hangul word carries a particle written onto it.
fn has_particle(token: &Token) -> bool {
    token.kind == TokenKind::Hangul
        && token.text.chars().count() > 1
        && token
            .text
            .chars()
            .next_back()
            .is_some_and(|c| KO_PARTICLES.contains(c))
}

/// Where the label that opens the clause `span` ends: just past the last
/// colon in it that a space or a line break follows, outside code, or the
/// clause's start when it opens with no label.
fn label_end(text: &str, code: &[Span], span: Span) -> usize {
    let slice = span.slice(text);
    let mut end = span.start;
    for (i, c) in slice.char_indices() {
        let at = span.start + i;
        if c == ':' && slice[i + 1..].starts_with(char::is_whitespace) {
            let region = code.partition_point(|r| r.end <= at);
            if !code.get(region).is_some_and(|r| r.start <= at) {
                end = at + 1;
            }
        }
    }
    end
}

/// Whether a Korean cue is written as the topic of its clause, the way
/// 나머지는 opens "나머지는 그대로 유지해": it labels what the rest of the
/// clause says.
fn topic(text: &str, hit: &Hit<CueId>) -> bool {
    let ends = |c: char| c == '은' || c == '는';
    let mut after = text[hit.span.end..].chars();
    match (hit.term.chars().next_back(), after.next(), after.next()) {
        (Some(last), next, _) if ends(last) => !next.is_some_and(yp_lang::legible::is_any_hangul),
        (_, Some(p), next) if ends(p) => !next.is_some_and(yp_lang::legible::is_any_hangul),
        _ => false,
    }
}

fn key(hit: &Hit<CueId>) -> (CueId, String) {
    (hit.id, hit.term.to_lowercase())
}

/// A name as compared across clauses: a word in any case, a name as written.
fn name(token: &Token) -> String {
    if token.kind == TokenKind::Word {
        token.text.to_lowercase()
    } else {
        token.text.clone()
    }
}

/// What `token` contributes. `order` lists `settled` in text order and
/// `starts` their start offsets; `verbs` is every action word the matcher
/// found, sorted.
fn role(
    text: &str,
    token: &Token,
    settled: &[Hit<CueId>],
    order: &[usize],
    starts: &[usize],
    verbs: &[Span],
) -> Role {
    if token.kind == TokenKind::Punct {
        // A number the tokenizer cannot read as one, -1 or 3.5, is still a
        // value a cue can take: "should return -1".
        return if token.text.chars().any(|c| c.is_ascii_digit()) {
            Role::Value
        } else {
            Role::Nothing
        };
    }
    if yp_lang::is_filler(token) {
        return Role::Nothing;
    }
    // The last hit starting before the token ends is the only one that can
    // overlap it: hits never overlap each other.
    let upto = starts.partition_point(|&s| s < token.span.end);
    let hit = order[..upto]
        .last()
        .map(|&i| &settled[i])
        .filter(|h| h.span.end > token.span.start);
    if let Some(hit) = hit {
        if is_evidence(hit.id) {
            if is_bound(hit.id) && verbs.binary_search(&hit.span).is_ok() {
                // A forbidden verb, read as the boundary it states.
                return Role::Content { bare: false };
            }
            let before = &text[token.span.start..hit.span.start.max(token.span.start)];
            let after = &text[hit.span.end.min(token.span.end)..token.span.end];
            if before
                .chars()
                .chain(after.chars())
                .any(|c| c.is_ascii_alphanumeric())
            {
                // The cue is part of a longer word or name: "except" in
                // "try-except", "response" in `response.content`.
                return Role::Content { bare: false };
            }
            return Role::Cue {
                particle: after.chars().any(yp_lang::legible::is_any_hangul),
                io: names_its_own(hit.id),
            };
        }
        match hit.id {
            CueId::ActionVerb => return Role::Objective,
            CueId::Question | CueId::Conjunction => return Role::Function,
            _ => {}
        }
    }
    // "a" and "I" are words before they are letters, unless quoted as values.
    if (is_article(token) || is_function_word(token)) && !quoted(text, token.span) {
        Role::Function
    } else if is_letter(text, token) {
        Role::Letter
    } else if is_value(text, token) {
        Role::Value
    } else {
        Role::Content {
            bare: matches!(token.kind, TokenKind::Word | TokenKind::Hangul),
        }
    }
}

/// Whether `token` is a value rather than a name: a number, written in
/// digits, with a unit, "30s", or as a word, or a quoted letter, `== 'A'`.
fn is_value(text: &str, token: &Token) -> bool {
    match token.kind {
        TokenKind::Number => true,
        TokenKind::Word | TokenKind::Hangul => {
            token.text.starts_with(|c: char| c.is_ascii_digit())
                || NUMBER_WORDS.binary_search(&token.text.as_str()).is_ok()
                || (token.text.chars().count() == 1 && quoted(text, token.span))
        }
        _ => false,
    }
}

fn is_function_word(token: &Token) -> bool {
    match token.kind {
        TokenKind::Word => {
            let word = token.text.to_ascii_lowercase();
            EN_FUNCTION_WORDS.binary_search(&word.as_str()).is_ok()
        }
        TokenKind::Hangul => KO_FUNCTION_WORDS
            .binary_search(&token.text.as_str())
            .is_ok(),
        _ => false,
    }
}

/// A single letter standing on its own: `x`, or the "b" of a list labelled
/// a, b, c. It stands in for a name rather than being one. Quoted, as in
/// `== 'A'`, it is a value.
fn is_letter(text: &str, token: &Token) -> bool {
    let mut chars = token.text.chars();
    let single = matches!((chars.next(), chars.next()), (Some(c), None) if c.is_alphabetic());
    single
        && matches!(
            token.kind,
            TokenKind::Word | TokenKind::Ident | TokenKind::CodeSpan
        )
        && !quoted(text, token.span)
}

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

/// The clauses of `text`, in order, covering it from start to end.
///
/// A clause ends at a comma, semicolon, full stop, exclamation or question
/// mark followed by a space or the end, and at a line break that starts a
/// list item or a paragraph or that follows the end of a sentence. Anything
/// inside code belongs to the clause around it, a clause ending in a colon
/// runs on into the next, and the full stop of "1." numbers a list item
/// rather than ending one.
fn clauses(text: &str) -> Vec<Span> {
    let code = code_regions(text);
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut region = 0usize;
    // Where the current line's first character other than a space is.
    let mut line: Option<usize> = None;
    // The last character other than a space in the clause so far. Found by
    // trimming the clause at every line break, it cost time that grew as the
    // square of the line breaks after a clause that ends in a colon: 64 KB of
    // them took the hook 0.7 s.
    let mut last: Option<char> = None;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '\n' {
            line = None;
        } else if line.is_none() && !c.is_whitespace() {
            line = Some(i);
        }
        while code.get(region).is_some_and(|r| r.end <= i) {
            region += 1;
        }
        let ends = !code.get(region).is_some_and(|r| r.start <= i) && {
            let next = chars.peek().map(|&(_, n)| n);
            let numbers = c == '.'
                && line
                    .is_some_and(|l| i - l <= 3 && text[l..i].bytes().all(|b| b.is_ascii_digit()));
            match c {
                _ if numbers => false,
                ',' | ';' | '.' | '!' | '?' => next.is_none_or(char::is_whitespace),
                '\n' => line_break_ends(&text[start..i], &text[i + 1..]),
                _ => false,
            }
        };
        if ends && last != Some(':') {
            let end = i + c.len_utf8();
            out.push(Span::new(start, end));
            start = end;
            last = None;
        } else if !c.is_whitespace() {
            last = Some(c);
        }
    }
    if start < text.len() || out.is_empty() {
        out.push(Span::new(start, text.len()));
    }
    out
}

/// Whether a line break ends the clause `before` it, given the text `after`.
fn line_break_ends(before: &str, after: &str) -> bool {
    let line = before.rsplit('\n').next().unwrap_or(before).trim();
    let next = after.split('\n').next().unwrap_or("").trim();
    line.is_empty()
        || next.is_empty()
        || line.ends_with(['.', '!', '?', ';', ','])
        || crate::stats::is_list_line(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// `text` as the pipeline settles it, and what is judged of it.
    fn judged(text: &str) -> (Vec<Token>, Vec<Hit<CueId>>, Verdict) {
        let tokens = yp_lang::tokenize(text);
        let found = yp_lang::resources().unwrap().cue_matcher.find(text);
        let settled = crate::negation::settle(text, &found);
        let (settled, found) = crate::nouns::strip(text, &tokens, found, settled);
        let verdict = judge(text, &tokens, &found, &settled);
        (tokens, settled, verdict)
    }

    /// The cue terms of `text` that are padding.
    fn padded(text: &str) -> Vec<String> {
        let (_, settled, verdict) = judged(text);
        settled
            .iter()
            .zip(verdict.hits)
            .filter(|(_, padding)| *padding)
            .map(|(h, _)| h.term.clone())
            .collect()
    }

    /// How many of the tokens of `text` nothing backs.
    fn unbacked_in(text: &str) -> usize {
        let (tokens, _, verdict) = judged(text);
        unbacked(&tokens, &verdict)
    }

    fn pieces(text: &str) -> Vec<&str> {
        clauses(text).iter().map(|s| s.slice(text).trim()).collect()
    }

    fn assert_kept(texts: &[&str]) {
        for text in texts {
            assert!(padded(text).is_empty(), "{text}: {:?}", padded(text));
        }
    }

    #[test]
    fn the_word_lists_are_sorted_so_the_search_works() {
        for list in [
            EN_FUNCTION_WORDS,
            KO_FUNCTION_WORDS,
            IO_NOUNS,
            FORMATS,
            VALUE_CUES,
            NUMBER_WORDS,
            STATEMENTS,
        ] {
            let mut sorted = list.to_vec();
            sorted.sort_unstable();
            assert_eq!(list, sorted.as_slice());
        }
    }

    #[test]
    fn a_cue_about_a_name_counts() {
        assert_kept(&[
            "In src/auth/login.rs, change verify_token so it returns Err(AuthError::Expired) \
             instead of panicking on an expired token. Don't change the public signature of \
             login(). tests/auth.rs must still pass.",
            "src/auth/login.rs 의 verify_token 이 만료된 토큰에서 panic 하는 대신 \
             Err(AuthError::Expired) 를 반환하도록 수정해줘. login() 의 공개 시그니처는 바꾸지 말 것. \
             tests/auth.rs 는 계속 통과해야 함.",
            "Requirements:\n- returns a Config struct\n- accepts --verbose\n- keeps the old flags",
            "Input: a list of integers. Output: the list sorted in descending order, without duplicates.",
            "Rename foo to bar in baz.rs. Keep qux unchanged. tests/baz.rs must still pass.",
            "Make the parser accept trailing commas. For example, `[1, 2, 3,]` should parse.",
            "If there's no such number, then the function should return -1.",
        ]);
    }

    #[test]
    fn cue_words_that_name_nothing_are_padding() {
        assert_eq!(
            padded("Refactor `x`. Returns json, input schema. Tests pass, assert, verify. Only, keep, unchanged."),
            ["Returns", "json", "input", "schema", "Tests pass", "assert", "verify", "Only", "keep", "unchanged"]
        );
        assert_eq!(
            padded("리팩터링해줘. `x` 반환, 입력, 출력, 스키마. 테스트 통과, 커버리지, 기대값. 에서만, 그대로, 유지해."),
            ["반환", "입력", "출력", "스키마", "테스트 통과", "커버리지", "기대값", "에서만", "그대로", "유지해"]
        );
    }

    #[test]
    fn a_statement_about_the_whole_task_counts() {
        // What it says is about the whole task, and it needs no name.
        assert_kept(&[
            "Refactor parse_args in src/cli.rs to use clap. Make sure all tests pass.",
            "Fix the flaky retry in src/net/client.rs. Done means: the test suite passes.",
            "Change the default timeout in client.go from 30s to 10s. Nothing else.",
            "Remove the debug prints. Keep everything else unchanged.",
            "Add a config loader. Acceptance criteria: all tests pass.",
            "status 명령에 --json 플래그 추가해줘. 나머지는 건드리지 마.",
            "디버그 출력을 지워줘. 나머지는 그대로 유지해.",
            "버그 고쳐줘. 모든 테스트가 통과해야 하고 다른 건 건드리지 마.",
        ]);
        // A criterion stated whole counts however many there are; a second
        // phrase of cue words in the same category says nothing new.
        assert_kept(&[
            "Fix login. Tests pass, no warnings.",
            "parser.py 버그 고쳐줘. 테스트 통과, 경고 없이.",
            "Fix the parser. Make sure all tests pass. No new warnings.",
        ]);
        assert_eq!(
            padded("Fix the parser. Make sure all tests pass. Verify it all."),
            ["Verify"]
        );
        // A cue that only marks a criterion, or names a kind of test, states
        // nothing on its own, and a run of cue words of two kinds is a list.
        assert_eq!(
            padded("Fix it. Tests pass. Make sure. Must pass."),
            ["Make sure", "Must pass"]
        );
        assert_eq!(
            padded("버그 고쳐줘. 테스트 통과. 완료 조건. 회귀 테스트."),
            ["완료 조건", "회귀 테스트"]
        );
        assert_eq!(padded("Fix it. Tests pass only."), ["Tests pass", "only"]);
    }

    #[test]
    fn an_io_cue_counts_in_a_sentence_and_not_in_a_list() {
        // The response, the output, JSON: I/O cue words name the result, so a
        // sentence of them says something without a name beside it.
        assert_kept(&[
            "Change the endpoint so the response is JSON.",
            "Make the output JSON.",
            "Update the schema.",
            "Document the input, output, and schema of each endpoint.",
            "Write the handler.\nInput: the request body\nOutput: the response",
            "출력은 JSON 형식으로 해줘.",
            "엔드포인트 응답을 JSON 으로 바꿔줘.",
        ]);
        // Two of them with nothing named are not about anything, unless one
        // is a format, which says what the other looks like.
        assert_eq!(
            padded("Refactor. Returns json, input schema."),
            ["input", "schema"]
        );
        // In a list they say nothing, format or not, and an article only
        // opens the noun phrase.
        assert_eq!(
            padded("Refactor. Returns the json, the input schema, the output yaml."),
            ["Returns", "json", "input", "schema", "output", "yaml"]
        );
        assert_eq!(
            padded("리팩터링해줘. 반환, 입력, 출력."),
            ["반환", "입력", "출력"]
        );
    }

    #[test]
    fn a_format_says_what_the_io_cue_beside_it_says() {
        assert_kept(&[
            "Add a `yp export` command. JSON output. Tests pass.",
            "Add a `yp export` command. Returns JSON. Tests pass.",
            "Add a `yp export` command, JSON output, tests pass.",
            "Add a config loader. YAML input. Tests pass.",
            "Add a /stats endpoint. JSON response. Tests pass.",
            "Add a `yp export` command.\nRequirements:\n- JSON output\n- tests pass",
            "`yp export` 명령 추가해줘. JSON 출력. 테스트 통과.",
            "`yp export` 명령 추가해줘. JSON 반환. 테스트 통과.",
        ]);
        // Two formats, or no format, say nothing of each other.
        assert_eq!(
            padded("Add a `yp export` command. JSON YAML."),
            ["JSON", "YAML"]
        );
        assert_eq!(
            padded("Add a `yp export` command. Input schema."),
            ["Input", "schema"]
        );
    }

    #[test]
    fn a_part_of_the_result_can_be_what_a_boundary_is_about() {
        assert_kept(&[
            "Keep the response schema unchanged.",
            "Keep the output unchanged.",
            "Keep the return values unchanged.",
            "Keep the return types unchanged.",
            "Don't change the response.",
            "입력과 출력은 그대로 유지해.",
            // With nothing else beside them, a part of the result and a
            // boundary are about each other.
            "Fix the expired-token panic in verify_token. Return type unchanged.",
            "Fix the parser. Signature unchanged.",
        ]);
        // A check is not about a part of the result for being beside it.
        assert_eq!(
            padded("Fix the parser. Tests pass input."),
            ["Tests pass", "input"]
        );
    }

    #[test]
    fn every_io_noun_the_lexicons_list_names_a_part_of_the_result_here() {
        // The I/O cue words that name no part of the result: verbs, words
        // that join a condition to its outcome, arrows, and phrases that
        // state a shape rather than name one. Any other I/O cue word is a
        // noun a boundary or a check can be about. "return values" was added
        // to the lexicon without being added here, and "Keep the return
        // values unchanged." lost its boundary and its result shape, 322.6
        // against 700.3 for "return value".
        const NAME_NOTHING: &[&str] = &[
            "->",
            "=>",
            "accepts",
            "expects",
            "for input",
            "given",
            "must return",
            "produces",
            "returns",
            "should return",
            "takes",
            "the shape",
            "then",
            "type is",
            "typed as",
            "when",
            "yields",
            "결과는",
            "결과로",
            "내보내",
            "돌려주",
            "돌려줘야",
            "리턴",
            "리턴해",
            "받아서",
            "반환",
            "반환해",
            "타입으로",
            "타입은",
            "형식으로",
            "형태로",
        ];
        let table = &yp_lang::resources().unwrap().cues.table;
        let terms: Vec<String> = table
            .terms
            .iter()
            .zip(&table.categories)
            .filter(|(_, &id)| id == CueId::IoSpec)
            .map(|(t, _)| t.to_lowercase())
            .collect();
        assert!(terms.len() > 40, "{} I/O cue words", terms.len());
        for term in &terms {
            assert!(
                IO_NOUNS.binary_search(&term.as_str()).is_ok()
                    ^ NAME_NOTHING.contains(&term.as_str()),
                "{term:?} must be in exactly one of IO_NOUNS and NAME_NOTHING"
            );
        }
        for noun in IO_NOUNS {
            assert!(
                terms.contains(&noun.to_string()),
                "{noun:?} is no I/O cue word"
            );
        }
        for format in FORMATS {
            assert!(IO_NOUNS.contains(format), "{format:?} is no I/O noun");
        }
        // The other lists name cue words of the categories judged here.
        let bound: Vec<String> = table
            .terms
            .iter()
            .zip(&table.categories)
            .filter(|(_, &id)| is_bound(id))
            .map(|(t, _)| t.to_lowercase())
            .collect();
        for term in VALUE_CUES.iter().chain(STATEMENTS) {
            assert!(bound.contains(&term.to_string()), "{term:?} is no cue word");
        }
    }

    #[test]
    fn a_list_of_cue_words_states_nothing() {
        // However the entries are separated, and with an article or a
        // function word apiece.
        for text in [
            "Fix it. Tests pass, assert, verify.",
            "Fix it. Tests pass; assert; verify",
            "Fix it.\n- tests pass\n- assert\n- verify",
            "Fix it. The tests pass, the assert, the verify.",
            "Fix it. Tests pass. Assert all. Verify all.",
            "버그 고쳐줘. 테스트 통과, 커버리지, 기대값.",
        ] {
            let padding = padded(text);
            assert_eq!(padding.len(), 3, "{text}: {padding:?}");
        }
        // A clause of six cue words with nothing named is a list whatever
        // holds it together.
        let text = "리팩터링해줘 모든 테스트가 건드리지 마 다른 건 반환 `x` 리턴 테스트 통과";
        assert_eq!(padded(text).len(), 6, "{:?}", padded(text));
    }

    #[test]
    fn a_value_names_only_the_cue_that_takes_it() {
        // A cue that takes a value is about it, written after the cue or, as
        // Korean writes it, before.
        assert_kept(&[
            "Add a /health endpoint. Returns 200.",
            "Fix the CLI. Exit code 1.",
            "When the token is expired, the endpoint should return 401.",
            "Write square(x). Input 5, output 25.",
            "Write add(a, b). add(2, 3) => 5.",
            "/health 엔드포인트 추가해줘. 200 을 반환해.",
        ]);
        // Another cue is not about a number, a number word or a quoted letter
        // beside it, nor about a number with a unit.
        for text in [
            "Fix login. Verify 7.",
            "Fix login. Verify seven.",
            "Fix login. Verify 'g'.",
            "Fix login. Verify 7ms.",
            "Fix login. Verify -7.",
        ] {
            assert_eq!(padded(text), ["Verify"], "{text}");
        }
    }

    #[test]
    fn a_run_of_cues_each_beside_one_bare_word_or_value_is_a_list() {
        // A different number, number word, quoted letter or plain word after
        // each cue word made the entries of a list look like statements about
        // something: after "Refactor verify_token." the list took the sample
        // to 915.3, an S, written in digits, words or Korean, 930.2 as
        // bullets, 899.1 with quoted letters and 825.1 with plain words. Two
        // values after each cue, or an "and" between them, are no different.
        let all = |text: &str| {
            let (_, settled, verdict) = judged(text);
            let bound = settled.iter().filter(|h| is_bound(h.id)).count();
            assert!(bound >= 9, "{text}");
            assert_eq!(
                verdict.hits.iter().filter(|&&p| p).count(),
                bound,
                "{text}: {:?}",
                padded(text)
            );
        };
        for text in [
            "Refactor `x`. Returns 1, input 2, schema 3. Tests pass 4, assert 5, verify 6. Only 7, keep 8, unchanged 9.",
            "Refactor `x`. Returns one, input two, schema three. Tests pass four, assert five, verify six. Only seven, keep eight, unchanged nine.",
            "Refactor `x`.\n- returns 1\n- input 2\n- schema 3\n- tests pass 4\n- assert 5\n- verify 6\n- only 7\n- keep 8\n- unchanged 9",
            "Refactor `x`. Returns 'a', input 'b', schema 'c'. Tests pass 'd', assert 'e', verify 'f'. Only 'g', keep 'h', unchanged 'i'.",
            "Refactor `x`. Returns red, input blue, schema green. Tests pass apple, assert pear, verify plum. Only north, keep south, unchanged east.",
            "Refactor `x`. Returns 1 2, input 3 4, schema 5 6. Tests pass 7 8, assert 9 10, verify 11 12. Only 13 14, keep 15 16, unchanged 17 18.",
            "Refactor `x`. Returns 1 and 2, input 3 and 4, schema 5 and 6. Tests pass 7 and 8, assert 9 and 10, verify 11 and 12. Only 13 and 14, keep 15 and 16, unchanged 17 and 18.",
            "리팩터링해줘 `x`. 반환 1, 입력 2, 스키마 3. 테스트 통과 4, 커버리지 5, 기대값 6. 에서만 7, 그대로 8, 유지해 9.",
            "리팩터링해줘 `x`. 반환 하나, 입력 둘, 스키마 셋. 테스트 통과 넷, 커버리지 다섯, 기대값 여섯. 에서만 일곱, 그대로 여덟, 유지해 아홉.",
        ] {
            all(text);
        }
        // One or two of them after the task are what they say, and so is a
        // checklist that names one cue word again and again.
        assert_kept(&[
            "Add a `yp export` command. JSON output. Return type unchanged. Tests pass.",
            "Add login tests.\n- verify login\n- verify logout\n- verify refresh",
        ]);
    }

    #[test]
    fn a_name_vouches_for_one_cue_however_often_it_appears() {
        let text = "Refactor `foo`. Verify `foo`. Assert `foo`. Ensure `foo`.";
        assert_eq!(padded(text), ["Assert", "Ensure"]);
        // Distinct names each vouch for their own.
        assert_kept(&["Verify `foo`. Assert `bar`. Ensure `baz`."]);
    }

    #[test]
    fn a_forbidden_verb_names_what_it_forbids() {
        assert_kept(&[
            "Don't change anything. Tests must still pass.",
            "아무것도 변경하지 말 것. 테스트는 계속 통과해야 해.",
        ]);
    }

    #[test]
    fn a_heading_or_a_list_number_names_nothing() {
        assert_eq!(padded("Refactor `x`.\nScope:\n- only"), ["only"]);
        assert_kept(&["Refactor `x`.\nScope: only src/ui."]);
        assert_eq!(
            padded("Refactor `x`.\n1. returns\n2. verify"),
            ["returns", "verify"]
        );
        assert_kept(&["Write load_config.\n1. Accept a path\n2. Return the parsed config"]);
    }

    #[test]
    fn cue_words_back_clarity_only_as_far_as_other_words_back_them() {
        // Every cue phrase here has a word beside it.
        for text in [
            "Fix login. Tests pass, no warnings.",
            "Don't change anything. Tests must still pass.",
            "Fix the flaky retry in src/net/client.rs. Done means: the test suite passes.",
            "출력은 그대로 유지해.",
        ] {
            assert_eq!(unbacked_in(text), 0, "{text}");
        }
        // Here nothing but the objective does; a letter backs nothing.
        assert_eq!(
            unbacked_in("Refactor `x`. Returns json, input schema. Tests pass, assert, verify. Only, keep, unchanged."),
            11
        );
        // Articles that open padding, and the words of a clause of nothing but
        // padding, back nothing either.
        assert_eq!(
            unbacked_in("Refactor. The tests pass, the assert, the verify."),
            unbacked_in("Refactor. Tests pass, assert, verify.") + 3
        );
        assert_eq!(
            unbacked_in("Refactor. Only all, keep all, unchanged all."),
            unbacked_in("Refactor. Only, keep, unchanged.") + 3
        );
        // Nor does the number of an item of nothing but padding; the number
        // of a real one counts as it always has.
        assert_eq!(
            unbacked_in("Refactor.\n1. assert\n2. verify"),
            unbacked_in("Refactor.\n- assert\n- verify") + 2
        );
        assert_eq!(unbacked_in("Steps:\n1. sign up\n2. log in"), 0);
    }

    #[test]
    fn clauses_end_where_phrases_do() {
        assert_eq!(
            pieces("Done means: the test suite passes. Keep it, then ship."),
            [
                "Done means: the test suite passes.",
                "Keep it,",
                "then ship."
            ]
        );
        // A docstring wrapped mid-sentence is one clause; a list item, a
        // blank line and a finished sentence end one, and the number of an
        // item does not.
        assert_eq!(
            pieces("returns the\n    biggest number\n- first\n2. second\n\nExpected output:\n```\n42\n```"),
            ["returns the\n    biggest number", "- first", "2. second", "", "Expected output:\n```\n42\n```"]
        );
        // Code is never cut, and "e.g." or 3.5 inside a sentence does not end
        // it.
        assert_eq!(
            pieces("Call `f(a, b)` with 3.5. Then stop"),
            ["Call `f(a, b)` with 3.5.", "Then stop"]
        );
        // A label reads on over blank lines and spaces, however they end.
        assert_eq!(
            pieces("Fix it:\n\n\nKeep it. Then stop"),
            ["Fix it:\n\n\nKeep it.", "Then stop"]
        );
        assert_eq!(
            pieces("Fix it: \r\n\t\r\n Keep it, then stop"),
            ["Fix it: \r\n\t\r\n Keep it,", "then stop"]
        );
    }

    #[test]
    fn a_long_salad_costs_about_what_plain_text_does() {
        // Every lookup is a binary search or a hash, so judging a long run of
        // cue words grows as reading it does. Timed against the same length
        // of ordinary words, the two measure how the work grows, not how fast
        // the machine is.
        let time = |text: &str| {
            let tokens = yp_lang::tokenize(text);
            let found = yp_lang::resources().unwrap().cue_matcher.find(text);
            (0..3)
                .map(|_| {
                    let start = Instant::now();
                    let verdict = judge(text, &tokens, &found, &found);
                    std::hint::black_box(unbacked(&tokens, &verdict));
                    start.elapsed()
                })
                .min()
                .unwrap()
        };
        let plain = time(&"parse the config file and print its name to stdout ".repeat(4_000));
        let names: String = (0..4_000)
            .map(|i| format!("verify name_{i} only "))
            .collect();
        let items: String = (0..4_000)
            .map(|i| format!("\n{}. keep the json", i % 999))
            .collect();
        for (label, text) in [
            (
                "one clause",
                "returns json input schema verify assert only keep the ".repeat(4_000),
            ),
            ("many clauses", "returns, json, input, `x`, ".repeat(4_000)),
            ("many names", names),
            ("many labels", "Scope: only keep: the json. ".repeat(4_000)),
            ("many items", items),
            (
                "one long line",
                format!("{}x", " ".repeat(40_000)) + &" 1. a".repeat(4_000),
            ),
        ] {
            let salad = time(&text);
            assert!(
                salad < plain * 20 + Duration::from_millis(100),
                "{label} {salad:?}, plain {plain:?}"
            );
        }
    }

    #[test]
    fn line_breaks_after_a_colon_cost_what_they_do_after_a_full_stop() {
        // A clause that ends in a colon runs on into what follows, and every
        // line break after one trimmed the whole clause again to see whether
        // it still ended in the colon. 64 KB of them took the hook 0.7 s, and
        // the same text with a full stop for the colon 30 ms. Timed against
        // that full stop, each shape measures how the work grows, not how
        // fast the machine is.
        let time = |text: &str| {
            let tokens = yp_lang::tokenize(text);
            let found = yp_lang::resources().unwrap().cue_matcher.find(text);
            (0..3)
                .map(|_| {
                    let start = Instant::now();
                    std::hint::black_box(judge(text, &tokens, &found, &found));
                    start.elapsed()
                })
                .min()
                .unwrap()
        };
        for gap in ["\n", "\r\n", "\n\t"] {
            let fill = gap.repeat(65_000 / gap.len());
            let colon = time(&format!(
                "Keep the output unchanged. Fix verify_token:{fill}x"
            ));
            let stop = time(&format!(
                "Keep the output unchanged. Fix verify_token.{fill}x"
            ));
            assert!(
                colon < stop * 20 + Duration::from_millis(100),
                "{gap:?} after a colon took {colon:?}, after a full stop {stop:?}"
            );
        }
    }
}
