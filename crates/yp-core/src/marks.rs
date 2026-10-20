//! Where in the text each point was won or lost.
//!
//! The axes report totals and one sentence each. That is enough to argue with
//! a score, but not enough to see it while typing: which word cost the
//! clarity points, which name the repository never heard of, which clause
//! earned the acceptance credit. A mark is one byte range of the user's own
//! text with a verdict attached, so a page can colour the prompt in place.
//!
//! Marks are drawn from the same hits and referents the axes were scored on.
//! They never contain a judgement the axes did not make; they only say where.

use std::borrow::Cow;
use std::collections::HashMap;

use serde::Serialize;
use yp_lang::{CueId, Hit, SmellId, Span, Token};

use crate::grounding::Referent;
use crate::prompt::Parts;

/// Whether a marked range helped, hurt, or leaves a question open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    Good,
    Warn,
    Bad,
}

/// One range of the original prompt text and what the scorer made of it.
#[derive(Debug, Clone, Serialize)]
pub struct Mark {
    /// Byte offsets into the text as the caller gave it, UTF-8, on character
    /// boundaries.
    pub start: usize,
    pub end: usize,
    pub tone: Tone,
    /// A stable machine id: `smell:ambiguous_adverb`, `cue:action_verb`,
    /// `cue:question`, `name:unique`, `name:one_file`, `name:ambiguous`,
    /// `name:missing`, `pronoun:dangling`, `pronoun:anchored`,
    /// `pronoun:pasted`, `attachment`. Pages translate these; the note is the
    /// English fallback.
    pub kind: String,
    /// For `name:ambiguous`, how many things the name could denote.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    pub note: String,
}

/// Everything the axes looked at, handed over so marks agree with them.
pub(crate) struct Evidence<'a> {
    pub parts: &'a Parts<'a>,
    pub tokens: &'a [Token],
    pub cue_hits: &'a [Hit<CueId>],
    pub smell_hits: &'a [Hit<SmellId>],
    /// Pronouns taken to point at pasted material, and so not charged.
    pub anchored: &'a [Span],
    /// Pronouns taken, with no repository to check against, to mean a
    /// code-shaped name before them in their sentence, and so not charged.
    pub named: &'a [Span],
    /// `None` when there was no repository to resolve names against.
    pub referents: Option<&'a [Referent]>,
    /// How many things the prompt asks for; a conjunction only costs when it
    /// joins a second one.
    pub objectives: usize,
}

/// The note on a pronoun with a name before it, grounded or not.
const ANCHORED: &str = "a pronoun; something named earlier may be what it means";

fn cue_note(id: CueId) -> &'static str {
    match id {
        CueId::ActionVerb => "names the action",
        CueId::IoSpec => "says what the result looks like",
        CueId::Acceptance => "says how to tell it is done",
        CueId::ScopeConstraint => "bounds the work",
        CueId::ExampleMarker => "introduces an example",
        CueId::Conjunction => "joins a second objective; one request per prompt scores higher",
        CueId::Question => "asks a direct question; the answer is the objective",
    }
}

fn push(
    out: &mut Vec<Mark>,
    span: Span,
    tone: Tone,
    kind: impl Into<String>,
    note: impl Into<String>,
) {
    out.push(Mark {
        start: span.start,
        end: span.end,
        tone,
        kind: kind.into(),
        count: None,
        note: note.into(),
    });
}

/// The most of a name a note repeats, in characters.
///
/// A name can run as long as the prompt does. Quoted whole under every part
/// of a 64 KB `a::a::…`, the marks came to 718 MB. Of the 2,946 names the
/// notes on SWE-bench Lite quote, scored against each issue's own
/// repository, the longest is 78 characters, a temporary file's path, so
/// none of those is cut.
const NOTE_NAME_CHARS: usize = 80;

/// `name` as a note quotes it: whole, or its first [`NOTE_NAME_CHARS`]
/// characters and an ellipsis.
fn clipped(name: &str) -> Cow<'_, str> {
    match name.char_indices().nth(NOTE_NAME_CHARS) {
        Some((cut, _)) => Cow::Owned(format!("{}…", &name[..cut])),
        None => Cow::Borrowed(name),
    }
}

/// The token that starts at `offset`, if one does.
///
/// Tokens come in text order, each starting after the one before it, so this
/// is a binary search.
fn token_at(tokens: &[Token], offset: usize) -> Option<&Token> {
    let at = tokens.partition_point(|t| t.span.start < offset);
    tokens.get(at).filter(|t| t.span.start == offset)
}

pub(crate) fn collect(ev: &Evidence<'_>) -> Vec<Mark> {
    let mut out = Vec::new();

    // The first name the repository knows is the earliest thing a pronoun
    // can point back at. Same rule as the deixis component.
    let anchor = ev.referents.and_then(crate::grounding::first_anchor);

    for hit in ev.smell_hits {
        if hit.id == SmellId::VaguePronoun {
            if let Some(referents) = ev.referents {
                let _ = referents;
                let dangling = anchor.is_none_or(|first| hit.span.start < first);
                if dangling {
                    // Nothing before it is in the repository, which is not
                    // the same as nothing named: in "change gravity() so it
                    // returns" the name right before "it" is made up.
                    push(
                        &mut out,
                        hit.span,
                        Tone::Bad,
                        "pronoun:dangling",
                        "points at nothing in your code",
                    );
                } else {
                    push(&mut out, hit.span, Tone::Warn, "pronoun:anchored", ANCHORED);
                }
                continue;
            }
        }
        push(
            &mut out,
            hit.span,
            Tone::Bad,
            format!("smell:{}", hit.id.as_str()),
            hit.id.label(),
        );
    }

    for &span in ev.anchored {
        push(
            &mut out,
            span,
            Tone::Good,
            "pronoun:pasted",
            "points at the pasted material",
        );
    }

    for &span in ev.named {
        push(&mut out, span, Tone::Warn, "pronoun:anchored", ANCHORED);
    }

    for hit in ev.cue_hits {
        if hit.id == CueId::Conjunction {
            if ev.objectives >= 2 {
                push(
                    &mut out,
                    hit.span,
                    Tone::Warn,
                    "cue:conjunction",
                    cue_note(hit.id),
                );
            }
            continue;
        }
        push(
            &mut out,
            hit.span,
            Tone::Good,
            format!("cue:{}", hit.id.as_str()),
            cue_note(hit.id),
        );
    }

    if let Some(referents) = ev.referents {
        // Several referents can share one span: the names inside a pasted
        // snippet all carry the snippet's offset. The span shows the worst of
        // them, which is the one the agent would trip on.
        //
        // Both the token under a referent and the span's entry used to be
        // found by scanning, once per referent: 64 KB of `,\`, 32,768 paths
        // the repository lacks, took the hook 671 ms against the demo's
        // sample that way, and takes 51 ms like this.
        debug_assert!(ev.tokens.is_sorted_by(|a, b| a.span.start < b.span.start));
        let mut worst: Vec<(Span, &Referent)> = Vec::new();
        // Where in `worst` each span is, by the offset it starts at: no two
        // tokens start at one offset.
        let mut held_at: HashMap<usize, usize> = HashMap::new();
        for r in referents {
            if let Some(&k) = held_at.get(&r.offset) {
                let held = &mut worst[k].1;
                if held.resolution() > r.resolution() {
                    *held = r;
                }
                continue;
            }
            let Some(token) = token_at(ev.tokens, r.offset) else {
                continue;
            };
            held_at.insert(r.offset, worst.len());
            worst.push((token.span, r));
        }
        for (span, r) in worst {
            let inside = span.slice(ev.parts.instruction()) != r.text;
            let name = if inside {
                format!("\"{}\" ", clipped(&r.text))
            } else {
                String::new()
            };
            match r.candidates() {
                // One file resolves as well as one definition does, but a
                // field like `expires_at`, or a `to_string` the code only
                // calls, has no definition the index can point to.
                Some(1) if r.facts.is_some_and(|f| f.def == 0) => push(
                    &mut out,
                    span,
                    Tone::Good,
                    "name:one_file",
                    format!(
                        "{name}appears in one file in this repository, with no definition found"
                    ),
                ),
                Some(1) => push(
                    &mut out,
                    span,
                    Tone::Good,
                    "name:unique",
                    format!("{name}is defined once in this repository"),
                ),
                Some(n) if n > 1 => out.push(Mark {
                    start: span.start,
                    end: span.end,
                    tone: Tone::Warn,
                    kind: "name:ambiguous".into(),
                    count: Some(n),
                    note: format!("{name}could be any of {n}"),
                }),
                _ if r.explicit => push(
                    &mut out,
                    span,
                    Tone::Bad,
                    "name:missing",
                    format!("{name}is not in this repository"),
                ),
                _ => {}
            }
        }
    }

    // Everything above is in instruction coordinates. Put it back on the
    // text the user is looking at, and drop anything that landed on a marker.
    let mut marks: Vec<Mark> = out
        .into_iter()
        .filter_map(|m| {
            let span = ev.parts.to_original(Span::new(m.start, m.end))?;
            Some(Mark {
                start: span.start,
                end: span.end,
                ..m
            })
        })
        .collect();

    for span in ev.parts.attachment_spans() {
        push(
            &mut marks,
            span,
            Tone::Good,
            "attachment",
            "attached material, judged as evidence rather than as a claim",
        );
    }

    // Leftmost first, longest first on a tie; anything overlapping an earlier
    // mark is dropped so a page can paint them as a flat sequence.
    marks.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
    let mut flat: Vec<Mark> = Vec::with_capacity(marks.len());
    for m in marks {
        if m.start == m.end {
            continue;
        }
        if flat.last().is_some_and(|last| m.start < last.end) {
            continue;
        }
        flat.push(m);
    }
    flat
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::MapCorpus;

    fn marks(text: &str) -> Vec<Mark> {
        crate::score(text).unwrap().marks
    }

    fn kinds(marks: &[Mark]) -> Vec<&str> {
        marks.iter().map(|m| m.kind.as_str()).collect()
    }

    #[test]
    fn a_vague_word_is_marked_bad_and_a_verb_good() {
        let text = "roughly fix the parser";
        let m = marks(text);
        let roughly = m
            .iter()
            .find(|m| &text[m.start..m.end] == "roughly")
            .unwrap();
        assert_eq!(roughly.tone, Tone::Bad);
        assert!(roughly.kind.starts_with("smell:"));
        let fix = m.iter().find(|m| &text[m.start..m.end] == "fix").unwrap();
        assert_eq!(fix.tone, Tone::Good);
        assert_eq!(fix.kind, "cue:action_verb");
    }

    #[test]
    fn marks_never_overlap_and_land_on_char_boundaries() {
        let text = "그거 좀 적당히 알아서 빨리 고쳐줘, 테스트가 통과하면 끝";
        let m = marks(text);
        assert!(!m.is_empty());
        for w in m.windows(2) {
            assert!(w[0].end <= w[1].start, "{:?} overlaps {:?}", w[0], w[1]);
        }
        for mark in &m {
            assert!(text.is_char_boundary(mark.start));
            assert!(text.is_char_boundary(mark.end));
            assert!(mark.start < mark.end);
        }
    }

    #[test]
    fn names_are_judged_against_the_repository() {
        let corpus = MapCorpus::new(100, &[("verify_token", 3, 12, 1), ("login", 37, 90, 0)]);
        let text = "fix verify_token in login and payroll.rs";
        let s = crate::score_with(text, Some(&corpus)).unwrap();
        let by_text = |t: &str| {
            s.marks
                .iter()
                .find(|m| &text[m.start..m.end] == t)
                .unwrap_or_else(|| panic!("no mark on {t}: {:?}", kinds(&s.marks)))
        };
        assert_eq!(by_text("verify_token").kind, "name:unique");
        assert_eq!(by_text("payroll.rs").kind, "name:missing");
        assert_eq!(by_text("payroll.rs").tone, Tone::Bad);
    }

    #[test]
    fn a_pronoun_before_any_name_dangles_and_after_one_does_not() {
        let corpus = MapCorpus::new(100, &[("verify_token", 3, 12, 1)]);
        let text = "fix it in verify_token, then rename it";
        let s = crate::score_with(text, Some(&corpus)).unwrap();
        let pronouns: Vec<&str> = s
            .marks
            .iter()
            .filter(|m| &text[m.start..m.end] == "it")
            .map(|m| m.kind.as_str())
            .collect();
        assert_eq!(pronouns, ["pronoun:dangling", "pronoun:anchored"]);
    }

    #[test]
    fn without_an_index_a_code_shaped_name_anchors_the_pronoun_after_it() {
        // The same verdicts as the grounded case above, read from the shape
        // of the name instead of from the repository.
        let text = "fix it in verify_token, then rename it";
        let m = marks(text);
        let pronouns: Vec<(&str, Tone)> = m
            .iter()
            .filter(|m| &text[m.start..m.end] == "it")
            .map(|m| (m.kind.as_str(), m.tone))
            .collect();
        assert_eq!(
            pronouns,
            [
                ("smell:vague_pronoun", Tone::Bad),
                ("pronoun:anchored", Tone::Warn)
            ]
        );
    }

    #[test]
    fn a_noun_is_not_marked_as_the_action() {
        let text = "add a test for verify_token";
        let m = marks(text);
        let on = |word: &str| -> Vec<&str> {
            m.iter()
                .filter(|m| &text[m.start..m.end] == word)
                .map(|m| m.kind.as_str())
                .collect()
        };
        assert_eq!(on("add"), ["cue:action_verb"]);
        assert!(on("test").is_empty());
    }

    #[test]
    fn offsets_survive_an_attachment_being_cut_out() {
        let body = "fn a() {}\n".repeat(20);
        let text = format!("fix the parser\n```rust\n{body}```\nroughly please");
        let m = marks(&text);
        let roughly = m
            .iter()
            .find(|m| &text[m.start..m.end] == "roughly")
            .expect("the word after the attachment is still found");
        assert_eq!(roughly.tone, Tone::Bad);
        let att = m
            .iter()
            .find(|m| m.kind == "attachment")
            .expect("attachment marked");
        assert!(text[att.start..att.end].starts_with("```"));
        assert!(att.start > text.find("parser").unwrap());
        assert!(att.end < roughly.start);
    }

    #[test]
    fn nothing_marked_in_gibberish() {
        assert!(marks("asdf qwer zxcv").is_empty());
    }

    #[test]
    fn a_forbidden_verb_is_not_marked_as_the_action() {
        let on = |text: &str, word: &str| -> Vec<String> {
            marks(text)
                .into_iter()
                .filter(|m| &text[m.start..m.end] == word)
                .map(|m| m.kind)
                .collect()
        };
        let text = "Fix the parser. Don't change the tests.";
        assert_eq!(on(text, "Fix"), ["cue:action_verb"]);
        assert_eq!(on(text, "Don't"), ["cue:scope_constraint"]);
        assert!(on(text, "change").is_empty());
        // Where no other word carries the boundary, the verb does.
        assert_eq!(
            on("no need to rename the docs", "rename"),
            ["cue:scope_constraint"]
        );
        assert_eq!(on("변경하지 말 것", "변경하"), ["cue:scope_constraint"]);
    }

    #[test]
    fn a_question_word_is_marked_as_the_objective() {
        let text = "Why does verify_token panic on expired tokens?";
        let why = marks(text)
            .into_iter()
            .find(|m| &text[m.start..m.end] == "Why")
            .expect("the question word is marked");
        assert_eq!(why.kind, "cue:question");
        assert_eq!(why.tone, Tone::Good);
    }

    #[test]
    fn a_pronoun_with_a_paste_to_point_at_is_marked_good_on_every_path() {
        const TRACE: &str = "Traceback (most recent call last):\n  \
                    File \"app/main.py\", line 42, in <module>\n    run()\n  \
                    File \"app/main.py\", line 30, in run\n    token = verify_token(raw)\n  \
                    File \"app/auth.py\", line 12, in verify_token\n    \
                    raise ExpiredError(\"token expired\")\nauth.ExpiredError: token expired\n";
        let pasted = Some(("pronoun:pasted".to_string(), Tone::Good));
        let kind_of_this = |text: &str, parts: &crate::prompt::Parts<'_>| {
            crate::score_parts(parts, None)
                .unwrap()
                .marks
                .iter()
                .find(|m| &text[m.start..m.end] == "this")
                .map(|m| (m.kind.clone(), m.tone))
        };

        // Long enough to be cut out as an attachment, on the page as in the
        // hook.
        let long = format!("fix this\n```\n{TRACE}```");
        let parts = crate::prompt::from_parts(&long, &[]);
        assert!(parts.has_attachments());
        assert_eq!(kind_of_this(&long, &parts), pasted);

        // Its first three lines are short enough to stay in the instruction.
        let short_trace = TRACE.lines().take(3).collect::<Vec<_>>().join("\n");
        let short = format!("fix this\n```\n{short_trace}\n```");
        let parts = crate::prompt::from_parts(&short, &[]);
        assert!(!parts.has_attachments());
        assert_eq!(kind_of_this(&short, &parts), pasted);

        // Opened as a file beside what was typed.
        let parts = crate::prompt::from_parts("fix this", &[TRACE]);
        assert_eq!(kind_of_this("fix this", &parts), pasted);
    }

    #[test]
    fn defined_once_is_said_only_of_a_definition() {
        let corpus = MapCorpus::new(
            6,
            &[
                ("verify_token", 4, 6, 1),
                ("expires_at", 1, 2, 0),
                ("token", 5, 20, 1),
            ],
        );
        let text = "set expires_at in verify_token";
        let s = crate::score_with(text, Some(&corpus)).unwrap();
        let on = |word: &str| {
            s.marks
                .iter()
                .find(|m| &text[m.start..m.end] == word)
                .unwrap_or_else(|| panic!("no mark on {word}: {:?}", kinds(&s.marks)))
        };
        assert_eq!(on("verify_token").kind, "name:unique");
        assert_eq!(
            on("verify_token").note,
            "is defined once in this repository"
        );
        // A field: one file holds it, and the index sees no definition.
        let field = on("expires_at");
        assert_eq!(field.kind, "name:one_file");
        assert_eq!(field.tone, Tone::Good);
        assert!(!field.note.contains("defined once"), "{}", field.note);
        // Both resolve, as the resolution component says.
        let resolution = &s.grounding.as_ref().unwrap().components[0];
        assert!(
            resolution.detail.starts_with("all 2 name(s) resolve"),
            "{}",
            resolution.detail
        );
    }

    #[test]
    fn a_pronoun_after_a_made_up_name_points_at_nothing_in_the_code() {
        // "it" comes straight after the name it means; the name is not here.
        let corpus = MapCorpus::new(6, &[("verify_token", 4, 6, 1)]);
        let text = "Refactor parse_config and make it faster.";
        let s = crate::score_with(text, Some(&corpus)).unwrap();
        let it = s
            .marks
            .iter()
            .find(|m| &text[m.start..m.end] == "it")
            .expect("the pronoun is marked");
        assert_eq!(it.kind, "pronoun:dangling");
        assert_eq!(it.note, "points at nothing in your code");
    }
}
