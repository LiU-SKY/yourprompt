//! Replies that only make sense inside a running conversation.
//!
//! "yes", "continue", "ok do it", "계속해줘": each answers the previous turn
//! and asks for nothing of its own. Grading one grades the conversation, not
//! the prompt, and in a live session every one of them replaced a real task's
//! score with an F -- "continue" scored 0.0, "yes" 15.1, "ok do it" 24.5.
//!
//! The test is all or nothing. A prompt is a follow-up only when every word in
//! it is a listed reply. Fillers, punctuation and emoji are skipped; anything
//! else -- a name, a path, a pasted snippet, an action verb, a word in another
//! script -- makes it a task again. Silently not grading a real request is the
//! worse mistake, so the lists hold only words that cannot ask for work on
//! their own, and one stray word is enough to fall back to grading.

use std::collections::HashMap;
use std::fmt;

use serde::Deserialize;

use crate::legible::{is_filler, is_laughter};
use crate::token::{tokenize, Token, TokenKind};

/// Stands for a choice, so "2", "option 2" and "2번" are each listed once
/// rather than once per number. A letter from a lettered list is a choice
/// too: "b", "(b)", "option b".
pub const CHOICE: &str = "#";

/// Longest number read as a choice. "2" picks an option; "2024" says
/// something.
const CHOICE_DIGITS: usize = 2;

/// Letters read as a choice, the way "b" and "option b" answer a list of
/// lettered options. Every letter here is a word that can make up a skipped
/// prompt on its own, so the list stops at d.
const CHOICE_LETTERS: &[&str] = &["a", "b", "c", "d"];

/// Marks that put a letter in code or in quotes, where it picks nothing:
/// "a = [1, 3, 7]", ">>> 2@B", "'a'". Read one at a time, 17 of the 9,384
/// lines of SWE-bench Lite and HumanEvalComm text became replies through a
/// bare letter; with these marks, 2 still do, rows of a printed matrix.
const CODE_MARKS: &[char] = &[
    '=', '@', '*', '<', '>', '[', ']', '{', '}', '|', '\'', '"', '`', '\\', '^', '$', '%', '&',
];

/// The one reply made of punctuation and a digit: agreement, as on a review
/// thread.
const PLUS_ONE: &str = "+1";

/// A follow-up is short. Past this many units a prompt is something else,
/// and is not broken up any further, so a pasted page of Korean is not split
/// into a unit per character only to be ruled out by its first word. A long
/// reply such as "네 알겠습니다 그럼 계속 진행해 주세요 감사합니다" is 21.
const MAX_UNITS: usize = 40;

/// One group of replies as it appears in a follow-up TOML file.
///
/// The id documents what the group is for. Matching treats every term alike,
/// except that a statement is no reply when it is asked.
#[derive(Debug, Clone, Deserialize)]
pub struct ReplyDef {
    pub id: String,
    pub terms: Vec<String>,
    /// The terms state something, so asked as a question they ask for
    /// something: "done" answers the previous turn, "done?" wants to know
    /// whether the work is finished, and is graded like any question.
    #[serde(default)]
    pub statement: bool,
}

/// A compiled term: its units, and whether it is a statement.
#[derive(Debug, Clone)]
struct Phrase {
    units: Vec<String>,
    statement: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FollowUpFile {
    pub lang: String,
    #[serde(rename = "reply", default)]
    pub replies: Vec<ReplyDef>,
}

/// A term no prompt could match: it carries a name or code, which no reply
/// may, is longer than a reply can be, or is empty.
#[derive(Debug)]
pub struct BadReply(pub String);

impl fmt::Display for BadReply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "follow-up term cannot be matched: {:?}", self.0)
    }
}

impl std::error::Error for BadReply {}

/// One matchable piece of text: a Latin word, one Hangul character, or a
/// choice number.
///
/// Korean is matched a character at a time so that "계속해줘", "계속 해줘"
/// and "계속해 줘" are the same reply. Casual spacing is not something a
/// follow-up should depend on.
struct Unit<'a> {
    text: &'a str,
    /// Fillers may be skipped: they are neither a reply nor a reason the
    /// prompt is not one.
    filler: bool,
}

fn is_choice(digits: &str) -> bool {
    (1..=CHOICE_DIGITS).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit())
}

/// Break tokens into units, or `None` at the first token no reply contains,
/// or when a letter that would be a choice stands among code marks.
fn units(tokens: &[Token]) -> Option<Vec<Unit<'_>>> {
    let mut out = Vec::new();
    let mut lettered = false;
    let mut coded = false;
    for token in tokens {
        let filler = is_filler(token);
        match token.kind {
            TokenKind::Word if CHOICE_LETTERS.contains(&token.text.as_str()) => {
                lettered = true;
                out.push(Unit {
                    text: CHOICE,
                    filler: false,
                })
            }
            TokenKind::Word => out.push(Unit {
                text: &token.text,
                filler,
            }),
            // Laughter typed onto a reply, "넵ㅎㅎ", is skipped like
            // laughter typed after it.
            TokenKind::Hangul => out.extend(token.text.char_indices().map(|(i, c)| Unit {
                text: &token.text[i..i + c.len_utf8()],
                filler: filler || is_laughter(c),
            })),
            TokenKind::Number if is_choice(&token.text) => out.push(Unit {
                text: CHOICE,
                filler: false,
            }),
            TokenKind::Punct if token.text == PLUS_ONE => out.push(Unit {
                text: PLUS_ONE,
                filler: false,
            }),
            TokenKind::Punct
                if token
                    .text
                    .strip_prefix(CHOICE)
                    .is_some_and(|n| n.is_empty() || is_choice(n)) =>
            {
                out.push(Unit {
                    text: CHOICE,
                    filler: false,
                })
            }
            // Punctuation and emoji say nothing either way. A letter the
            // tokenizer could not place -- kana, Cyrillic, an accented Latin
            // letter -- is a word in some language, and not a listed one;
            // digits are an issue number or a quantity, not a choice.
            TokenKind::Punct if !token.text.chars().any(char::is_alphanumeric) => {
                coded |= token.text.contains(CODE_MARKS);
            }
            // A name, a path, a pasted snippet, a number too long to be a
            // choice.
            _ => return None,
        }
        if out.len() > MAX_UNITS {
            return None;
        }
    }
    (!(lettered && coded)).then_some(out)
}

/// Every bundled reply, compiled for matching.
#[derive(Debug, Clone, Default)]
pub struct FollowUps {
    /// Each term as a sequence of units, grouped by its first unit.
    by_first: HashMap<String, Vec<Phrase>>,
}

impl FollowUps {
    /// Compile the terms of several files into one matcher.
    ///
    /// Terms are tokenized exactly as prompts are, so a term and the prompt
    /// that types it can never disagree about where the words are.
    pub fn from_files(files: &[FollowUpFile]) -> Result<Self, BadReply> {
        let mut by_first: HashMap<String, Vec<Phrase>> = HashMap::new();
        for file in files {
            for def in &file.replies {
                for term in &def.terms {
                    let term = term.trim();
                    if term.is_empty() {
                        continue;
                    }
                    let tokens = tokenize(term);
                    let phrase: Vec<String> = match units(&tokens) {
                        Some(units) if !units.is_empty() => {
                            units.iter().map(|u| u.text.to_string()).collect()
                        }
                        _ => return Err(BadReply(term.to_string())),
                    };
                    by_first.entry(phrase[0].clone()).or_default().push(Phrase {
                        units: phrase,
                        statement: def.statement,
                    });
                }
            }
        }
        Ok(Self { by_first })
    }

    /// Every compiled term, as its units.
    pub fn phrases(&self) -> impl Iterator<Item = &[String]> {
        self.by_first
            .values()
            .flatten()
            .map(|phrase| phrase.units.as_slice())
    }

    /// True when every word of the text is one of these replies.
    ///
    /// Fillers may be skipped, but at least one reply has to be there: "hmm"
    /// on its own, or a line of punctuation, is not a reply to anything.
    pub fn is_follow_up(&self, tokens: &[Token]) -> bool {
        let Some(units) = units(tokens) else {
            return false;
        };
        let asked = tokens
            .iter()
            .any(|t| t.kind == TokenKind::Punct && matches!(t.text.as_str(), "?" | "\u{FF1F}"));
        let n = units.len();
        // reach[i]: 0 when nothing ends at unit i, 1 when only skipped
        // fillers do, 2 when a run of replies does.
        let mut reach = vec![0u8; n + 1];
        reach[0] = 1;
        let mut furthest = 0;
        for i in 0..n {
            // Nothing reaches this unit, so nothing reaches past it either:
            // the word here is not part of any reply.
            if i > furthest {
                return false;
            }
            let here = reach[i];
            if here == 0 {
                continue;
            }
            if units[i].filler {
                reach[i + 1] = reach[i + 1].max(here);
                furthest = furthest.max(i + 1);
            }
            for phrase in self.by_first.get(units[i].text).into_iter().flatten() {
                if asked && phrase.statement {
                    continue;
                }
                let end = i + phrase.units.len();
                if end <= n
                    && phrase
                        .units
                        .iter()
                        .zip(&units[i..end])
                        .all(|(p, u)| p.as_str() == u.text)
                {
                    reach[end] = 2;
                    furthest = furthest.max(end);
                }
            }
        }
        reach[n] == 2
    }
}

pub const EN_TOML: &str = include_str!("../../../data/followups.en.toml");
pub const KO_TOML: &str = include_str!("../../../data/followups.ko.toml");

pub fn load_bundled() -> Result<FollowUps, Box<dyn std::error::Error>> {
    let en: FollowUpFile = toml::from_str(EN_TOML)?;
    let ko: FollowUpFile = toml::from_str(KO_TOML)?;
    Ok(FollowUps::from_files(&[en, ko])?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cues::{self, CueId};
    use crate::matcher::Matcher;

    fn replies() -> FollowUps {
        load_bundled().expect("bundled follow-ups must load")
    }

    fn is_follow_up(text: &str) -> bool {
        replies().is_follow_up(&tokenize(text))
    }

    #[test]
    fn both_languages_parse_and_keep_the_same_groups() {
        // Parity: a group added for one language gets its counterpart in the
        // other, or this fails.
        let en: FollowUpFile = toml::from_str(EN_TOML).unwrap();
        let ko: FollowUpFile = toml::from_str(KO_TOML).unwrap();
        assert_eq!(en.lang, "en");
        assert_eq!(ko.lang, "ko");
        let ids = |f: &FollowUpFile| {
            let mut v: Vec<String> = f.replies.iter().map(|r| r.id.clone()).collect();
            v.sort();
            v
        };
        assert_eq!(ids(&en), ids(&ko));
        for file in [&en, &ko] {
            for reply in &file.replies {
                assert!(!reply.terms.is_empty(), "{} has no terms", reply.id);
            }
        }
    }

    #[test]
    fn no_term_is_listed_twice_in_one_language() {
        // Korean spacing is ignored, so "한번 더" and "한 번 더" would be the
        // same term; this catches that kind of duplicate too.
        for toml in [EN_TOML, KO_TOML] {
            let file: FollowUpFile = toml::from_str(toml).unwrap();
            let lang = file.lang.clone();
            let compiled = FollowUps::from_files(&[file]).unwrap();
            let mut seen = std::collections::HashSet::new();
            for phrase in compiled.phrases() {
                assert!(
                    seen.insert(phrase.to_vec()),
                    "{lang} lists {phrase:?} twice"
                );
            }
        }
    }

    #[test]
    fn a_term_that_names_something_is_refused() {
        let file = FollowUpFile {
            lang: "en".into(),
            replies: vec![ReplyDef {
                id: "bad".into(),
                terms: vec!["fix src/main.rs".into()],
                statement: false,
            }],
        };
        assert!(FollowUps::from_files(&[file]).is_err());
    }

    #[test]
    fn an_action_verb_is_only_ever_a_whole_reply() {
        // "retry" is an action verb in cues.en.toml and a reply here: alone
        // it can only mean the last thing again. What may never happen is an
        // action verb inside a longer reply, which is how "fix it" would get
        // past the classifier.
        let cues = cues::load_bundled().unwrap();
        let matcher = Matcher::new(&cues.table).unwrap();
        for file in [EN_TOML, KO_TOML] {
            let file: FollowUpFile = toml::from_str(file).unwrap();
            for term in file.replies.iter().flat_map(|r| &r.terms) {
                for hit in matcher.find(term) {
                    if hit.id == CueId::ActionVerb {
                        assert_eq!(
                            hit.term.as_str(),
                            term.as_str(),
                            "{term:?} has the action verb {:?} in it",
                            hit.term
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn words_that_ask_for_work_are_never_replies() {
        for word in [
            "fix",
            "add",
            "make",
            "change",
            "update",
            "remove",
            "delete",
            "test",
            "commit",
            "push",
            "undo",
            "revert",
            "it",
            "this",
            "that",
            "고쳐",
            "고쳐줘",
            "추가해",
            "만들어",
            "수정해",
            "바꿔",
            "그거",
            "이거",
            "저거",
            "그렇게",
        ] {
            assert!(!is_follow_up(word), "{word:?} passed as a reply");
        }
    }

    #[test]
    fn a_reply_is_short() {
        assert!(is_follow_up(
            "네 알겠습니다 그럼 계속 진행해 주세요 감사합니다"
        ));
        assert!(!is_follow_up(&"yes ".repeat(MAX_UNITS + 1)));
        assert!(!is_follow_up(&"계속".repeat(MAX_UNITS)));
    }

    #[test]
    fn spacing_does_not_matter_in_korean() {
        for text in [
            "계속해줘",
            "계속 해줘",
            "계속해 줘",
            "계속 해 줘",
            "다시해줘",
            "다시 해 줘",
        ] {
            assert!(is_follow_up(text), "{text:?}");
        }
    }

    #[test]
    fn fillers_punctuation_and_emoji_are_skipped_but_are_not_replies() {
        assert!(is_follow_up("ok!!"));
        assert!(is_follow_up("hmm... ok 👍"));
        assert!(is_follow_up("음 계속"));
        for text in ["hmm", "음...", "!!!", "👍", "", "   "] {
            assert!(!is_follow_up(text), "{text:?} is not a reply to anything");
        }
    }

    #[test]
    fn a_choice_is_a_small_number() {
        for text in ["2", "option 2", "2번", "#2", "1, 3", "2번으로 해줘"] {
            assert!(is_follow_up(text), "{text:?}");
        }
        for text in ["2024", "#123", "2.5", "123456 7890"] {
            assert!(!is_follow_up(text), "{text:?}");
        }
    }

    #[test]
    fn a_choice_is_a_letter_from_a_to_d() {
        for text in ["b", "B", "(b)", "b)", "option b", "go with c", "a and d"] {
            assert!(is_follow_up(text), "{text:?}");
        }
        for text in ["e", "option e", "plan b", "b2"] {
            assert!(!is_follow_up(text), "{text:?}");
        }
        // Beside an operator, a bracket or a quote, a letter is code or a
        // quotation, not a pick.
        for text in ["a = [1, 3, 7, 13, 21]", ">>> 2@B", "> A", "'a'", "['a']"] {
            assert!(!is_follow_up(text), "{text:?}");
        }
    }

    #[test]
    fn plus_one_agrees() {
        assert!(is_follow_up("+1"));
        assert!(is_follow_up("+1, go ahead"));
        assert!(!is_follow_up("+2"));
    }

    #[test]
    fn laughter_is_skipped_even_typed_onto_a_reply() {
        for text in ["넵 ㅎㅎ", "응 ㅋㅋ", "넵ㅎㅎ", "좋아요ㅋㅋㅋ", "ok ㅋㅋ"] {
            assert!(is_follow_up(text), "{text:?}");
        }
        // Laughter alone says nothing, and next to a request it hides none.
        for text in ["ㅋㅋㅋ", "ㅎㅎ", "고쳐줘ㅋㅋ"] {
            assert!(!is_follow_up(text), "{text:?}");
        }
    }

    #[test]
    fn a_statement_asked_is_a_question() {
        for text in ["done", "Done.", "ok done", "확인했어", "확인했어요"] {
            assert!(is_follow_up(text), "{text:?}");
        }
        for text in ["done?", "ok, done?", "확인했어?", "확인했어요？"] {
            assert!(!is_follow_up(text), "{text:?}");
        }
        // Other replies stay replies when asked, as they were.
        assert!(is_follow_up("ok?"));
    }

    #[test]
    fn a_stem_that_an_ending_turns_into_a_request_is_not_a_reply() {
        // Stems and endings combine, so bare "확인" would make "확인해줘",
        // a request to check something, a reply.
        for text in [
            "확인",
            "확인해줘",
            "확인해 줘",
            "다시 확인해줘",
            "체크해줘",
            "봐줘",
        ] {
            assert!(!is_follow_up(text), "{text:?}");
        }
        for text in [
            "계속해줘요",
            "진행 부탁드립니다",
            "다음으로 넘어가",
            "부탁해요",
        ] {
            assert!(is_follow_up(text), "{text:?}");
        }
    }

    #[test]
    fn a_letter_from_another_script_is_a_word_like_any_other() {
        // The tokenizer leaves kana, Cyrillic and accented Latin as single
        // characters. They are words nonetheless, and not listed ones.
        for text in ["ok はい", "ok да", "sí", "ok, más"] {
            assert!(!is_follow_up(text), "{text:?}");
        }
    }
}
