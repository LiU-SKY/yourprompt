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
    /// `name:unique`, `name:ambiguous`, `name:missing`, `pronoun:dangling`,
    /// `pronoun:anchored`, `attachment`. Pages translate these; the note is
    /// the English fallback.
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
    /// `None` when there was no repository to resolve names against.
    pub referents: Option<&'a [Referent]>,
    /// How many distinct action verbs the prompt has; a conjunction only
    /// costs when it joins a second one.
    pub objectives: usize,
}

fn cue_note(id: CueId) -> &'static str {
    match id {
        CueId::ActionVerb => "names the action",
        CueId::IoSpec => "says what the result looks like",
        CueId::Acceptance => "says how to tell it is done",
        CueId::ScopeConstraint => "bounds the work",
        CueId::ExampleMarker => "introduces an example",
        CueId::Conjunction => "joins a second objective; one request per prompt scores higher",
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
                    push(
                        &mut out,
                        hit.span,
                        Tone::Bad,
                        "pronoun:dangling",
                        "points at nothing named before it",
                    );
                } else {
                    push(
                        &mut out,
                        hit.span,
                        Tone::Warn,
                        "pronoun:anchored",
                        "a pronoun; something named earlier may be what it means",
                    );
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
        let mut worst: Vec<(Span, &Referent)> = Vec::new();
        for r in referents {
            let Some(token) = ev.tokens.iter().find(|t| t.span.start == r.offset) else {
                continue;
            };
            match worst.iter_mut().find(|(s, _)| *s == token.span) {
                Some((_, held)) if held.resolution() <= r.resolution() => {}
                Some((_, held)) => *held = r,
                None => worst.push((token.span, r)),
            }
        }
        for (span, r) in worst {
            let inside = span.slice(ev.parts.instruction()) != r.text;
            let name = if inside {
                format!("\"{}\" ", r.text)
            } else {
                String::new()
            };
            match r.candidates() {
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
}
