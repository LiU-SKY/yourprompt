//! The scoring engine: turns a prompt into a 0-1000 understandability score.
//!
//! Pure and deterministic. No I/O, no clock, no network, no model call -- the
//! same prompt always produces the same number, which is what makes the score
//! worth putting on screen every time the user presses enter.
//!
//! # The four axes
//!
//! | Axis | Max | Question |
//! |------|-----|----------|
//! | A grounding | 350 | Do the things this prompt names resolve to exactly one thing in *this* repository? |
//! | B actionability | 250 | Does it name one concrete action, a result shape, and a way to tell when it is done? |
//! | C clarity | 250 | How densely is it packed with ambiguity smells? |
//! | D context | 150 | Does it bound its own scope, show what it means, and come in a workable shape? |
//!
//! Axis A needs a repository index. When none is available the axis is
//! dropped and B, C and D are renormalised to fill the full 1000, so the
//! number stays comparable rather than silently capping at 650 -- but the
//! score is flagged as renormalised, because a grounded score and an
//! ungrounded one are not the same measurement.
//!
//! Only a task is graded. A slash command, a "yes" or a "continue", and a
//! prompt in a language the lexicons cannot read are scored all the same,
//! but [`Score::kind`] says the number does not describe them.

mod antecedents;
pub mod axes;
pub mod corpus;
pub mod grade;
pub mod grounding;
pub mod kind;
pub mod marks;
mod negation;
mod nouns;
pub mod params;
pub mod pasted;
pub mod prompt;
pub mod questions;
pub mod stats;

use serde::Serialize;

pub use axes::{Axis, Component};
pub use corpus::{Corpus, MapCorpus, TermFacts};
pub use kind::Kind;
pub use marks::{Mark, Tone};
pub use stats::PromptStats;

/// What a prompt is, read the way [`score_with`] reads it: the instruction
/// once pasted material is cut out, to its first
/// [`prompt::MAX_INSTRUCTION_BYTES`].
///
/// The hook asks this before deciding whether to score at all, so it reads
/// the text the score reads, and the two always agree. [`kind::kind_of`] reads
/// the raw prompt, and past the cap it can answer differently. The cap is not
/// what keeps this fast any more: a megabyte of short fenced blocks is
/// classified in 12.5 ms read raw and in 3.7 ms read like this.
pub fn kind_of(text: &str) -> (Kind, Option<&'static str>) {
    let parts = prompt::split(text);
    let capped = parts.capped();
    kind::kind_of(capped.as_ref().unwrap_or(&parts).instruction())
}

/// A scored prompt.
#[derive(Debug, Clone, Serialize)]
pub struct Score {
    /// 0.0 to 1000.0.
    pub total: f64,
    pub grade: &'static str,
    /// What the prompt is. Everything here is computed for every kind, but
    /// only a [`Kind::Task`] is meant to be shown as a grade.
    pub kind: Kind,
    /// For an unsupported prompt, the English name of the language it looks
    /// like, when that can be told. Always `None` for the other kinds.
    pub language: Option<&'static str>,
    /// Axis A. `None` when no repository index was available, in which case
    /// the remaining axes were renormalised to 1000.
    pub grounding: Option<Axis>,
    pub actionability: Axis,
    pub clarity: Axis,
    pub context: Axis,
    /// True when the total was scaled up to compensate for a missing axis A.
    /// Surfaced in the status line so a renormalised score is never mistaken
    /// for a grounded one.
    pub renormalized: bool,
    /// Where in the text each verdict was made: the words that cost clarity
    /// points, the names that did or did not resolve, the clauses that earned
    /// credit. Byte ranges into the text as the caller gave it.
    pub marks: Vec<Mark>,
}

impl Score {
    /// The total rounded for display. One decimal is enough to break ties
    /// without implying precision the model does not have.
    pub fn display_total(&self) -> f64 {
        (self.total * 10.0).round() / 10.0
    }
}

/// Split a pasted snippet into tokens.
///
/// The snippet has already had its backticks stripped, so it is read as plain
/// text: re-running code detection over it would mask the whole thing again.
pub fn tokenize_snippet(text: &str) -> Vec<yp_lang::Token> {
    yp_lang::tokenize(text)
}

/// The things a prompt names, resolved against a repository.
///
/// Exposed so callers can inspect what the grounding axis saw, rather than
/// only the number it produced.
pub fn referents_of(text: &str, corpus: &dyn Corpus) -> Vec<grounding::Referent> {
    let parts = prompt::split(text);
    let capped = parts.capped();
    let instruction = capped.as_ref().unwrap_or(&parts).instruction();
    let tokens = yp_lang::tokenize(instruction);
    grounding::referents(instruction, &tokens, &[], corpus)
}

/// Score a prompt.
///
/// Returns `None` only if the bundled language resources failed to load, which
/// should be impossible in a release build but is handled rather than
/// panicked on: this runs on every prompt the user types, and a crash here
/// would be a crash in their editor.
pub fn score(text: &str) -> Option<Score> {
    score_with(text, None)
}

/// Score a prompt against a repository.
///
/// With a corpus, axis A is scored and the total spans all four axes. Without
/// one, axis A is dropped and B, C and D are renormalised to fill the full
/// 1000, so a score is never silently capped at 650 -- but `renormalized` is
/// set so the two are never confused.
pub fn score_with(text: &str, corpus: Option<&dyn Corpus>) -> Option<Score> {
    // A raw blob may carry pasted material. Anything long enough to have been
    // pasted rather than typed is treated as an attachment, not as something
    // the user said.
    let parts = prompt::split(text);
    score_parts(&parts, corpus)
}

/// How much of the absence credit this prompt has earned the right to.
///
/// Clarity and deixis reward what a prompt *lacks* -- no vague words, no
/// unattached references. A text that asks for nothing lacks those defects
/// the way silence does, so the reward is cut to
/// [`params::clarity::NO_REQUEST_FACTOR`] unless something request-shaped is
/// present: an objective (an action verb or a direct question), a result
/// shape, an acceptance criterion, or a scope boundary. Requirements smells
/// presuppose a requirement.
pub(crate) fn request_factor(
    cues: &std::collections::BTreeMap<yp_lang::CueId, Vec<String>>,
) -> f64 {
    let requestish = [
        yp_lang::CueId::ActionVerb,
        yp_lang::CueId::Question,
        yp_lang::CueId::IoSpec,
        yp_lang::CueId::Acceptance,
        yp_lang::CueId::ScopeConstraint,
    ]
    .iter()
    .any(|id| cues.get(id).is_some_and(|hits| !hits.is_empty()));
    if requestish {
        1.0
    } else {
        params::clarity::NO_REQUEST_FACTOR
    }
}

/// Score a prompt whose attachments the caller has already separated.
///
/// The instruction is judged as an instruction; attachments are judged as
/// material. A prompt that carries the file it is about is a better prompt
/// than one that does not, and this is where that stops being penalised.
///
/// Only the start of a very long prompt is read; see
/// [`prompt::MAX_INSTRUCTION_BYTES`]. Every way in passes through here, so the
/// hook, the command line and the web page share the bound.
pub fn score_parts(parts: &prompt::Parts<'_>, corpus: Option<&dyn Corpus>) -> Option<Score> {
    let capped = parts.capped();
    let parts = capped.as_ref().unwrap_or(parts);
    let resources = yp_lang::resources()?;
    let text = parts.instruction();

    // One tokenisation and one code-region scan, shared by every axis.
    let tokens = yp_lang::tokenize(text);
    let pasted = pasted::Pasted::of(parts);
    let stats = stats::analyze(text, &tokens).with_pasted(pasted.substance());
    let code = yp_lang::token::code_regions(text);
    let (kind, language) = kind::classify(text, &tokens);

    let cue_hits = resources.cue_matcher.find_masked(text, &code);
    let smell_hits = resources.smells.find_masked(text, &code);

    // A "this" with a pasted block to point at is neither vague nor dangling,
    // and a direct question asks as plainly as an action verb does.
    let (smell_hits, anchored) = pasted.anchor(smell_hits);
    let (questions, smell_hits) = questions::asked(text, &tokens, smell_hits, pasted.is_material());

    // "change" in "don't change the schema" is a constraint, not the
    // objective. The axes and the marks read the settled hits; grounding
    // still takes every hit as an instruction word rather than a name.
    let settled = [negation::settle(text, &cue_hits), questions.clone()].concat();
    let cue_hits = [cue_hits, questions].concat();
    // "test" in "add a test" names the work rather than adding to it.
    let settled = nouns::strip(text, &cue_hits, settled);
    // With no index, a pronoun after a code-shaped name in its sentence is
    // taken to mean that name, as grounding takes it to mean one that resolves.
    let (smell_hits, named) = match corpus {
        Some(_) => (smell_hits, Vec::new()),
        None => antecedents::anchor(text, &tokens, smell_hits),
    };

    let cues = axes::distinct_by_category(&settled);
    let smells = axes::total_by_category(&smell_hits);
    let request = request_factor(&cues);

    let grounded = corpus.map(|corpus| {
        let offsets = grounding::pronoun_offsets(&smell_hits);
        // Spans already claimed by a cue or a smell are instruction words,
        // not names the user is pointing at.
        let instruction_words: Vec<yp_lang::Span> = cue_hits
            .iter()
            .map(|h| h.span)
            .chain(smell_hits.iter().map(|h| h.span))
            .chain(anchored.iter().copied())
            .collect();
        grounding::grounding(
            text,
            &tokens,
            &offsets,
            &instruction_words,
            &parts.attachments,
            corpus,
            stats.diversity() * request,
        )
    });
    let (grounding_axis, referents) = match grounded {
        Some((axis, referents)) => (Some(axis), Some(referents)),
        None => (None, None),
    };

    // With grounding active, vague pronouns are judged there -- by whether
    // they actually have an antecedent -- so clarity does not charge for them
    // a second time.
    let waived: &[yp_lang::SmellId] = if grounding_axis.is_some() {
        &[yp_lang::SmellId::VaguePronoun]
    } else {
        &[]
    };

    let actionability = axes::actionability(&cues);
    let clarity = axes::clarity(&smells, &resources.lexicon, &stats, waived, request);
    let context = axes::context(&cues, &stats);

    let rest = actionability.earned + clarity.earned + context.earned;
    let (total, renormalized) = match &grounding_axis {
        Some(axis) => (axis.earned + rest, false),
        None => (
            rest * (params::axis_max::TOTAL / params::axis_max::WITHOUT_GROUNDING),
            true,
        ),
    };
    let total = total.clamp(0.0, params::axis_max::TOTAL);

    let marks = marks::collect(&marks::Evidence {
        parts,
        tokens: &tokens,
        cue_hits: &settled,
        smell_hits: &smell_hits,
        anchored: &anchored,
        named: &named,
        referents: referents.as_deref(),
        objectives: axes::objectives(&cues),
    });

    Some(Score {
        total,
        grade: grade::grade(total),
        kind,
        language,
        grounding: grounding_axis,
        actionability,
        clarity,
        context,
        renormalized,
        marks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn total(text: &str) -> f64 {
        score(text).unwrap().total
    }

    #[test]
    fn scores_stay_inside_the_range_for_anything() {
        let texts = [
            "",
            " ",
            "?",
            "fix",
            "그거",
            "\u{0}\u{1}",
            "```\nunterminated",
            &"very ".repeat(5000),
            "🎉🎉🎉",
        ];
        for text in texts {
            let s = score(text).unwrap();
            assert!(
                (0.0..=1000.0).contains(&s.total),
                "{:?} scored {}",
                text,
                s.total
            );
            assert!(!s.grade.is_empty());
        }
    }

    #[test]
    fn make_is_an_action_like_its_korean_twin() {
        // A vague request is still a request: "make it work" keeps the
        // objective and pays for its vagueness in clarity, the way "좀
        // 봐줘" does.
        let objective = |text: &str| {
            score(text)
                .unwrap()
                .actionability
                .components
                .iter()
                .find(|c| c.id == "objective")
                .unwrap()
                .earned
        };
        assert!(objective("make the status line faster") > 0.0);
        assert!(objective("좀 더 빠르게 만들어줘") > 0.0);
        assert!(total("make it work") < total("make the status line faster"));
    }

    #[test]
    fn a_courtesy_adds_nothing() {
        assert_eq!(total("fix the bug"), total("fix the bug pls"));
        assert_eq!(total("fix the bug"), total("please fix the bug"));
    }

    #[test]
    fn a_forbidden_verb_asks_for_nothing_in_either_language() {
        // "Don't change anything. Tests must still pass." scored 673 with
        // "change" as its objective, and "dont" was not read at all.
        let earned = |text: &str, id: &str| {
            let s = score(text).unwrap();
            [&s.actionability, &s.context]
                .iter()
                .flat_map(|axis| &axis.components)
                .find(|c| c.id == id)
                .unwrap()
                .earned
        };
        for text in [
            "Don't change anything. Tests must still pass.",
            "asdf asdf src/a.rs must pass tests dont change",
            "아무것도 변경하지 말 것. 테스트는 계속 통과해야 해.",
        ] {
            assert_eq!(earned(text, "objective"), 0.0, "{text}");
            assert!(earned(text, "scope") > 0.0, "{text}");
        }
        // Beside a real request, the prohibition bounds it rather than
        // counting as a second one.
        let text = "Fix the parser. Don't change the tests.";
        assert_eq!(
            earned(text, "singularity"),
            params::actionability::SINGULARITY_MAX
        );
        assert!(
            total("Don't change anything. Tests must still pass.")
                < total("Fix the parser. Tests must still pass.")
        );
    }

    #[test]
    fn saying_what_done_means_is_an_acceptance_criterion() {
        // "Done means: the test suite passes" earned none of the 70
        // acceptance points; "The existing tests must still pass" earned 41.5.
        let acceptance = |text: &str| {
            let s = score(text).unwrap();
            s.actionability
                .components
                .iter()
                .find(|c| c.id == "acceptance")
                .unwrap()
                .earned
        };
        let done = acceptance(
            "Fix the flaky retry in src/net/client.rs. Done means: the test suite passes.",
        );
        let still = acceptance(
            "Fix the flaky retry in src/net/client.rs. The existing tests must still pass.",
        );
        assert!(
            done >= still,
            "done means {done} vs must still pass {still}"
        );
        // In Korean the criterion carries its own label, 완료 조건.
        assert!(
            acceptance("재시도를 고쳐줘. 완료 조건: 테스트가 전부 통과할 것")
                > acceptance("재시도를 고쳐줘. 테스트가 전부 통과할 것")
        );
    }

    #[test]
    fn running_something_is_a_request_in_both_languages() {
        // "run the tests" scored 65 with no objective at all.
        for text in [
            "run the tests",
            "execute the migration",
            "테스트 돌려줘",
            "테스트 실행해줘",
        ] {
            let s = score(text).unwrap();
            let objective = &s.actionability.components[0];
            assert_eq!(objective.id, "objective");
            assert!(objective.earned > 0.0, "{text}: {}", objective.detail);
        }
    }

    #[test]
    fn hype_costs_clarity_in_both_languages() {
        // "Write 10 viral hooks" from a "world-class copywriter" scored 570
        // with clarity at full marks.
        let clarity = |text: &str| score(text).unwrap().clarity.earned;
        for (hyped, plain) in [
            (
                "Act as a world-class copywriter. Write 10 viral hooks for my SaaS.",
                "Act as a senior copywriter. Write 10 short hooks for my SaaS.",
            ),
            (
                "월드클래스 카피라이터처럼 내 SaaS용 바이럴 훅 10개 작성해줘.",
                "시니어 카피라이터처럼 내 SaaS용 짧은 훅 10개 작성해줘.",
            ),
        ] {
            assert!(clarity(hyped) < clarity(plain), "{hyped}");
        }
    }

    #[test]
    fn scoring_is_deterministic() {
        let text =
            "refactor verify_token in src/auth/login.rs so it returns Result, tests must pass";
        let first = total(text);
        for _ in 0..20 {
            assert_eq!(total(text), first);
        }
    }

    #[test]
    fn a_grounded_specific_prompt_beats_a_vague_one() {
        let good = total(
            "In src/auth/login.rs, change verify_token so it returns \
             Result<Claims, AuthError> instead of panicking when the token is \
             expired. Update the two call sites in src/api/handlers.rs. Don't \
             change the public signature of login(). The existing tests in \
             tests/auth.rs must still pass.",
        );
        let bad = total("그냥 로그인 그거 좀 적당히 알아서 고쳐줘");
        assert!(good > bad * 1.5, "good {good} vs bad {bad}");
    }

    #[test]
    fn the_same_request_scores_higher_when_it_is_specified() {
        // The heart of the model: identical intent, different specificity.
        let vague = total("fix the login handler");
        let specific = total(
            "fix verify_token in src/auth/login.rs: it panics on an expired \
             token and should return Err(AuthError::Expired) instead. \
             tests/auth.rs::expired_token_is_rejected must pass.",
        );
        assert!(specific > vague, "specific {specific} vs vague {vague}");
    }

    #[test]
    fn keyword_stuffing_does_not_reach_the_top() {
        // Repeating every cue word we know should saturate, not max out.
        let stuffed = total(
            &"refactor implement add test verify assert returns input output \
              only don't for example "
                .repeat(40),
        );
        assert!(stuffed < 950.0, "stuffed prompt scored {stuffed}");
    }

    #[test]
    fn a_mashed_keyboard_scores_near_the_floor() {
        // Gibberish has no ambiguity smells and no dangling references,
        // because it has no words -- and it used to be paid for both. The
        // repeated form scored 356 with clarity at full marks.
        for mash in [
            "asdfasefawefasf zxdf2wq4rq235wrsadgㅁㄴㅇㄹ",
            &"asdfasefawefasf zxdf2wq4rq235wrsadg ㅁㄴㅇㄹㅎㅁㄴㅇㄻㄴㅇㄹ               qwerqwer zxcvzxcv asdfasdf "
                .repeat(6),
            &"asdfghjkl".repeat(60),
        ] {
            let s = score(mash).expect("scores");
            assert!(s.total < 120.0, "mash scored {}: {mash:.40}", s.total);
            assert_eq!(s.grade, "F");
        }
    }

    #[test]
    fn one_word_held_down_scores_near_the_floor() {
        // "아" over eight lines scored 346 and "테스트" ten times scored 389,
        // with clarity at full marks both times: legible tokens, enough of
        // them, and not a single ambiguity smell -- because between them they
        // say one word. Absence credit is now scaled by lexical variety and
        // by whether anything was asked for.
        for held in [
            "아아아아아아
아아아
아
아
아
아
아
아",
            "아아아아아아아아아아아아아아아아",
            "테스트 테스트 테스트 테스트 테스트 테스트 테스트 테스트 테스트 테스트",
            "가나가나가나가나 가나가나가나가나 가나가나가나가나",
            "음... 어... 그... 저... 음... 어... 그... 저... 음...",
            "123456 7890 1234 5678 90123 456789 012345",
        ] {
            let t = total(held);
            assert!(t < 150.0, "{held:.30} scored {t}");
        }
    }

    #[test]
    fn text_that_asks_for_nothing_scores_below_a_real_request() {
        // A greeting or a complaint has no ambiguity smells and no dangling
        // references because it makes no request in which either could occur.
        // It used to collect full marks for both absences and outscore actual
        // requests. Requirements smells presuppose a requirement.
        let request = total("그거 좀 고쳐줘");
        for non_request in [
            "안녕하세요 반갑습니다",
            "아 진짜 짜증나네 왜 자꾸 이러지 답답해 죽겠다",
            "와 진짜 대박이다 이거 미쳤네 진짜로",
            "고맙습니다 정말 감사합니다 덕분에 잘 됐어요",
        ] {
            let t = total(non_request);
            assert!(
                t < request,
                "{non_request} scored {t}, request scored {request}"
            );
        }
    }

    #[test]
    fn a_word_of_thanks_after_a_real_request_is_not_charged_for() {
        // The non-request discount must key on the absence of request cues,
        // not on the presence of pleasantries.
        let plain = total("crates 안의 parse_args 함수를 수정해줘");
        let polite = total("고마워! 이번에는 crates 안의 parse_args 함수를 수정해줘");
        assert!(polite > plain * 0.8, "polite {polite} vs plain {plain}");
    }

    #[test]
    fn the_korean_fixture_holds_its_ordering() {
        // bench/korean.txt is 32 Korean prompts in five tiers, from held-down
        // keys to a fully specified request. The scorer must keep tier 0
        // under 150 and tier 1 under 350, and must order at least 90% of
        // cross-tier pairs correctly. Scored ungrounded, so the result does
        // not drift as this repository grows.
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../bench/korean.txt");
        let text = std::fs::read_to_string(path).expect("bench/korean.txt");

        let mut records: Vec<(u32, String, f64)> = Vec::new();
        for block in text.split("\n---\n") {
            let block: String = block
                .lines()
                .skip_while(|l| l.starts_with('#') || l.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            let (head, body) = block.split_once('\n').expect("tiered record");
            let (tier, label) = head.split_once('\t').expect("tier<TAB>label");
            let tier: u32 = tier.trim().parse().expect("numeric tier");
            let total = score(body.trim_matches('\n')).expect("scores").total;
            records.push((tier, label.to_string(), total));
        }
        assert!(
            records.len() > 25,
            "fixture parsed {} records",
            records.len()
        );

        for (tier, label, total) in &records {
            let ceiling = match tier {
                0 => 150.0,
                1 => 350.0,
                _ => continue,
            };
            assert!(
                *total < ceiling,
                "tier {tier} '{label}' scored {total}, ceiling {ceiling}"
            );
        }

        let mut pairs = 0u32;
        let mut correct = 0u32;
        for (i, a) in records.iter().enumerate() {
            for b in &records[i + 1..] {
                if a.0 == b.0 {
                    continue;
                }
                pairs += 1;
                if (a.2 > b.2) == (a.0 > b.0) {
                    correct += 1;
                }
            }
        }
        let accuracy = f64::from(correct) / f64::from(pairs);
        assert!(
            accuracy >= 0.90,
            "cross-tier ordering {correct}/{pairs} = {:.1}%",
            accuracy * 100.0
        );
    }

    #[test]
    fn a_real_prompt_is_untouched_by_the_gibberish_guard() {
        // The claim is not about the total -- it is that the guard does not
        // engage at all for real prompts. It is a ratio, so technical
        // vocabulary the word list has never heard of costs nothing as long as
        // ordinary words hold the sentence together.
        let legibility_of = |text: &str| {
            let tokens = yp_lang::tokenize(text);
            stats::analyze(text, &tokens).legibility()
        };
        for text in [
            "Rewrite informativeness in crates/yp-core/src/grounding.rs so it              counts definition sites rather than document frequency.",
            "crates/yp-core/src/grounding.rs 의 informativeness 를 정의 위치              기준으로 다시 작성해줘",
            "fix the login handler",
            "`verify_token` panics on an expired token",
        ] {
            assert_eq!(
                legibility_of(text),
                1.0,
                "the guard engaged on a real prompt: {text:.50}"
            );
        }

        // And it does engage on mashing.
        assert!(legibility_of("asdfasefawefasf zxdf2wq4rq235wrsadg qwerqwer") < 0.3);
    }

    #[test]
    fn attaching_the_file_a_task_is_about_does_not_cost_points() {
        // The complaint that started this: a prompt carrying its own context
        // scored seventy points *below* the same prompt without it.
        let instruction = "Rewrite the parser in src/cli.rs so it returns a Config.";
        let file: String = (0..40)
            .map(|i| {
                format!(
                    "fn helper_{i}() {{ let value = {i}; }}
"
                )
            })
            .collect();
        let with_context = format!(
            "{instruction}

```rust
{file}```
"
        );

        let alone = total(instruction);
        let attached = total(&with_context);
        // Ungrounded, an attachment cannot help -- but it must not hurt.
        assert!(
            attached >= alone - 1.0,
            "attaching context cost {:.1} points",
            alone - attached
        );
    }

    #[test]
    fn empty_prompt_scores_near_the_floor() {
        assert!(total("") < 350.0);
    }

    #[test]
    fn display_total_rounds_to_one_decimal() {
        let s = score("refactor the parser").unwrap();
        let shown = s.display_total();
        assert!((shown * 10.0 - (shown * 10.0).round()).abs() < 1e-9);
    }

    #[test]
    fn score_serializes_to_json() {
        let s = score("refactor parse_args in src/cli.rs").unwrap();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"total\""));
        assert!(json.contains("\"actionability\""));
        assert!(json.contains("\"renormalized\":true"));
        assert!(json.contains("\"kind\":\"task\""), "{json}");
        assert!(json.contains("\"language\":null"), "{json}");

        let s = score("corrige le bug dans le parseur pour que les tests passent").unwrap();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"kind\":\"unsupported\""), "{json}");
        assert!(json.contains("\"language\":\"French\""), "{json}");
    }

    #[test]
    fn every_kind_is_still_scored() {
        // The kind says whether the number describes the prompt. It never
        // changes the number: what to show is the consumer's call.
        for (text, kind) in [
            ("refactor parse_args in src/cli.rs", Kind::Task),
            ("/compact keep the API notes", Kind::Command),
            ("ok do it", Kind::FollowUp),
            (
                "arregla el bug en src/auth/login.rs para que verify_token devuelva \
                 Err(AuthError::Expired). Los tests deben pasar.",
                Kind::Unsupported,
            ),
        ] {
            let s = score(text).unwrap();
            assert_eq!(s.kind, kind, "{text}");
            assert!((0.0..=1000.0).contains(&s.total), "{text}");
            assert_eq!(s.grade, grade::grade(s.total));
        }
    }

    #[test]
    fn classifying_a_prompt_agrees_with_scoring_it() {
        // The hook classifies a prompt before deciding whether to score it
        // at all, and reads what scoring reads: the instruction left once
        // attachments are cut out, to the same limit.
        let file = "fn helper() { let value = 1; }\n".repeat(30);
        let long = |line: &str| line.repeat(2 * prompt::MAX_INSTRUCTION_BYTES / line.len());
        // Spanish to the limit and an English request past it: read whole,
        // the English made it a task the score had not seen.
        let spanish_then_english = format!(
            "{}fix the parser",
            long("arregla el analizador para que acepte esta entrada. ")
        );
        assert_eq!(kind_of(&spanish_then_english).0, Kind::Unsupported);
        for text in [
            "continue".to_string(),
            format!("continue\n```rust\n{file}```"),
            format!("/score\n```\n{file}```"),
            format!("arregla el bug para que el parser no falle con esto:\n```rust\n{file}```"),
            "fix the parser".to_string(),
            long("fix the parser so it accepts this input. "),
            long("arregla el analizador para que acepte esta entrada. "),
            spanish_then_english,
            long("yes "),
            long("fix it\n```\ncode\n```\n"),
        ] {
            let s = score(&text).unwrap();
            assert_eq!(kind_of(&text), (s.kind, s.language), "{text:.40}");
        }
    }

    #[test]
    fn grounding_is_absent_until_the_index_exists() {
        let s = score("fix src/main.rs").unwrap();
        assert!(s.grounding.is_none());
        assert!(s.renormalized);
    }

    const TRACE: &str = "thread 'main' panicked at src/auth/login.rs:9:41:\n\
                         called `Result::unwrap()` on an `Err` value: ()\n\
                         note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace";

    fn fenced(instruction: &str, block: &str) -> String {
        format!("{instruction}\n```\n{block}\n```")
    }

    fn component(axis: &Axis, id: &str) -> f64 {
        axis.components.iter().find(|c| c.id == id).unwrap().earned
    }

    #[test]
    fn a_fenced_trace_scores_like_the_same_trace_pasted_bare() {
        // "fix this" above a fenced trace scored 278; the same trace without
        // the fence scored 559. The fence is what a careful user adds.
        let bare = total(&format!("fix this\n{TRACE}"));
        let fence = total(&fenced("fix this", TRACE));
        assert!(
            (fence - bare).abs() < 80.0,
            "fenced {fence:.1} vs bare {bare:.1}"
        );
        assert!(fence > total("fix this") + 200.0, "fenced {fence:.1}");
    }

    #[test]
    fn a_pronoun_before_the_block_points_at_it() {
        let text = fenced("fix this", TRACE);
        let s = score(&text).unwrap();
        assert!(!s.clarity.components[0].detail.contains("pronoun"));
        let this = s
            .marks
            .iter()
            .find(|m| &text[m.start..m.end] == "this")
            .expect("the pronoun is marked");
        assert_eq!(this.kind, "pronoun:pasted");
        assert_eq!(this.tone, Tone::Good);

        // Grounded, it is not dangling deixis either.
        let corpus = MapCorpus::new(10, &[("verify_token", 1, 3, 1)]);
        let grounded = score_with(&text, Some(&corpus)).unwrap();
        let deixis = grounded
            .grounding
            .unwrap()
            .components
            .into_iter()
            .find(|c| c.id == "deixis")
            .unwrap();
        assert_eq!(deixis.detail, "no unattached references");
    }

    #[test]
    fn a_fence_around_nothing_legible_changes_nothing() {
        // Mash, one word held down, a phrase on repeat, and underscored mash
        // that looks like identifiers: none of them gives "this" anything to
        // point at, or the prompt anything to stand on.
        let empty = total(&fenced("fix this", "x"));
        for block in [
            "asdfasefawefasf zxdf2wq4rq235wrsadg qwerqwer zxcvzxcv\nasdfasdf qwpeoriu zmxncbv",
            &"error error error error\n".repeat(4),
            &"the value of ".repeat(30),
            "asdf_qwer zxcv_uiop hjkl_bnm qwe_rty",
            "ㅁㄴㅇㄹ ㅋㅌㅊㅍ ㅂㅈㄷㄱ ㅁㄴㅇㄹㅎ",
        ] {
            let t = total(&fenced("fix this", block));
            assert_eq!(t, empty, "{block:.30} moved the score to {t:.1}");
        }
    }

    #[test]
    fn a_paste_counts_the_same_whichever_way_it_arrives() {
        // The hook splits the raw prompt; the web page hands over what was
        // typed. When the page kept a pasted fence inline, its names were
        // read as names the user had typed: the demo's showcase with its
        // sample's login.rs pasted under it scored 832.0 on the page and
        // 845.7 in the status line.
        let lines: String = (0..12)
            .map(|i| format!("    at handler_{i} (src/server/route_{i}.ts:{i}:5)\n"))
            .collect();
        let text = format!(
            "fix this\n```rust\nTypeError: Cannot read properties of undefined\n{lines}```"
        );
        assert!(prompt::split(&text).has_attachments());
        let corpus = MapCorpus::new(20, &[("handler_3", 1, 2, 1), ("route_3", 1, 1, 0)]);
        for corpus in [None, Some(&corpus as &dyn Corpus)] {
            let hook = score_with(&text, corpus).unwrap();
            let page = score_parts(&prompt::from_parts(&text, &[]), corpus).unwrap();
            assert_eq!(
                serde_json::to_string(&hook).unwrap(),
                serde_json::to_string(&page).unwrap()
            );
        }
        assert!(
            total(&text) > total("fix this") + 200.0,
            "got {}",
            total(&text)
        );
    }

    /// A prompt the scorer reads to its end, followed by `len` bytes of more
    /// of the same.
    fn padded(head: &str, filler: &str, len: usize) -> String {
        let mut text = head.to_string();
        while text.len() < len {
            text.push_str(filler);
        }
        text
    }

    #[test]
    fn only_the_start_of_a_very_long_prompt_is_read() {
        let head = "Fix verify_token in src/auth/login.rs so an expired token returns \
                    Err(AuthError::Expired). Tests must pass. ";
        let corpus = MapCorpus::new(10, &[("verify_token", 1, 3, 1)]);
        // Hangul is three bytes a character, so the cut lands inside one.
        for filler in [
            "the handler should log it and carry on. ",
            "로그를 남기고 계속해 ",
        ] {
            let long = padded(head, filler, 3 * prompt::MAX_INSTRUCTION_BYTES);
            let mut end = prompt::MAX_INSTRUCTION_BYTES;
            while !long.is_char_boundary(end) {
                end -= 1;
            }
            for corpus in [None, Some(&corpus as &dyn Corpus)] {
                let read = score_with(&long, corpus).unwrap();
                let head_only = score_with(&long[..end], corpus).unwrap();
                assert_eq!(read.total, head_only.total, "{filler}");
                for mark in &read.marks {
                    assert!(mark.end <= end, "{mark:?}");
                }
            }
        }
    }

    #[test]
    fn only_the_start_of_a_very_long_paste_is_read() {
        let block = |len: usize| padded("", "    at handler (src/server/route.ts:1:5)\n", len);
        let fenced = |len: usize| format!("fix this\n```\n{}```", block(len));
        let corpus = MapCorpus::new(10, &[("handler", 1, 3, 1), ("route", 1, 1, 0)]);
        let just_over = fenced(prompt::MAX_ATTACHED_BYTES + 100);
        let far_over = fenced(16 * prompt::MAX_ATTACHED_BYTES);
        for corpus in [None, Some(&corpus as &dyn Corpus)] {
            let near = score_with(&just_over, corpus).unwrap();
            let far = score_with(&far_over, corpus).unwrap();
            assert_eq!(near.total, far.total);
            // The marker still says that something was attached.
            let attachment = far.marks.iter().find(|m| m.kind == "attachment").unwrap();
            assert_eq!(attachment.end, far_over.len());
        }
    }

    #[test]
    fn a_cut_never_splits_an_attachment_marker() {
        // The instruction runs to just short of the limit, so the marker for
        // the paste after it would straddle it.
        let head = padded("fix the parser. ", "the parser should accept it. ", 0);
        let prose = padded(&head, "x", prompt::MAX_INSTRUCTION_BYTES - 5);
        let paste = "line of the paste\n".repeat(40);
        let text = format!("{prose}\n```\n{paste}```\nroughly");
        let parts = prompt::split(&text);
        let capped = parts.capped().unwrap();
        assert!(capped.instruction().len() <= prompt::MAX_INSTRUCTION_BYTES);
        assert!(
            !capped.instruction().contains('`'),
            "a partial marker survived"
        );
        let s = score_with(&text, None).unwrap();
        for mark in &s.marks {
            assert!(mark.start < mark.end && mark.end <= text.len(), "{mark:?}");
        }
    }

    #[test]
    fn a_direct_question_is_a_request() {
        // A specific, answerable question scored 155: no action verb, so no
        // objective, and clarity cut as if it asked for nothing.
        for question in [
            "Why does verify_token panic on expired tokens?",
            "verify_token이 왜 만료된 토큰에서 panic 나?",
        ] {
            let s = score(question).unwrap();
            assert_eq!(
                component(&s.actionability, "objective"),
                params::actionability::OBJECTIVE_MAX,
                "{question}"
            );
            assert!(s.total > 450.0, "{question} scored {}", s.total);
            assert!(
                s.marks.iter().any(|m| m.kind == "cue:question"),
                "{question}: {:?}",
                s.marks
            );
        }
    }

    #[test]
    fn a_vague_question_stays_in_the_f_range() {
        let specific = total("Why does verify_token panic on expired tokens?");
        for vague in [
            "why is it broken again?",
            "왜 안 돼?",
            "what is this?",
            "why?",
        ] {
            let t = total(vague);
            assert!(t < 350.0, "{vague} scored {t}");
            assert!(t < specific - 150.0, "{vague} {t} vs specific {specific}");
        }
    }

    #[test]
    fn a_question_adds_no_objective_to_an_action() {
        // "can you fix X?" asks for the fix; the question is only how it asks.
        let s = score("can you fix verify_token in src/auth/login.rs?").unwrap();
        assert_eq!(
            component(&s.actionability, "singularity"),
            params::actionability::SINGULARITY_MAX
        );
    }

    #[test]
    fn an_action_word_used_as_a_noun_is_not_a_second_objective() {
        // Each of these asks for one thing, and each kept between 16.3 and
        // 29.6 of the 40 singularity points. "add a run script" fell from
        // 527.1 to 511.1 when "run" became an action.
        for text in [
            "add a test for verify_token",
            "fix the failing test in tests/auth.rs",
            "add a run script to package.json",
            "make the test run faster",
            "the test run is flaky, fix the retry in tests/auth.rs",
        ] {
            let s = score(text).unwrap();
            assert_eq!(
                component(&s.actionability, "objective"),
                params::actionability::OBJECTIVE_MAX,
                "{text}"
            );
            assert_eq!(
                component(&s.actionability, "singularity"),
                params::actionability::SINGULARITY_MAX,
                "{text}: {}",
                s.actionability.components[1].detail
            );
        }
        // Two requests are still two.
        let s = score("Make the login return a 401 when the password is empty, and add a test.")
            .unwrap();
        assert!(
            component(&s.actionability, "singularity") < params::actionability::SINGULARITY_MAX
        );
    }

    #[test]
    fn a_statement_whose_only_action_word_is_a_noun_asks_for_nothing() {
        // "the build is broken" took "build" as its objective and scored
        // 356.4, above "fix the bug" at 305.4. The Korean has no such word to
        // misread, and scores 41.0.
        for text in ["the build is broken", "the test is flaky", "빌드가 깨졌어"] {
            let s = score(text).unwrap();
            assert_eq!(component(&s.actionability, "objective"), 0.0, "{text}");
            assert!(s.total < total("fix the bug"), "{text}: {}", s.total);
        }
    }

    #[test]
    fn a_pronoun_after_a_name_is_not_charged_without_an_index() {
        // The showcase's "it", right after verify_token, cost 38.5 of the 250
        // clarity points, and the page marked it as pointing at nothing.
        let real = "In src/auth/login.rs, change verify_token so it returns \
                    Err(AuthError::Expired) instead of panicking on an expired token. \
                    Don't change the public signature of login(). tests/auth.rs must still pass.";
        let made_up = "In src/banana.rs, change gravity() so it returns Err(MoonError::Full) \
                       instead of panicking on a full moon. Don't change the public signature \
                       of orbit(). tests/moon.rs must still pass.";
        let s = score(real).unwrap();
        assert_eq!(s.clarity.components[0].detail, "no ambiguity smells");
        let it = s
            .marks
            .iter()
            .find(|m| &real[m.start..m.end] == "it")
            .expect("the pronoun is marked");
        assert_eq!(
            (it.kind.as_str(), it.tone),
            ("pronoun:anchored", Tone::Warn)
        );
        // Unchecked, a made-up name anchors it as well as a real one.
        assert_eq!(total(made_up), s.total);

        // Nothing named before it in its sentence.
        for text in [
            "it's broken again",
            "fix it",
            "make it better",
            "Fix verify_token. It panics on expired tokens.",
        ] {
            let s = score(text).unwrap();
            assert!(
                s.clarity.components[0].detail.contains("vague pronoun"),
                "{text}: {}",
                s.clarity.components[0].detail
            );
        }
    }

    #[test]
    fn a_question_with_its_material_pasted_is_answerable() {
        let bare = total("Why does it panic?");
        let shown = total(&fenced("Why does it panic?", TRACE));
        assert!(shown > bare + 300.0, "shown {shown:.1} vs bare {bare:.1}");
        let ko = total(&fenced("이거 왜 안 돼?", TRACE));
        assert!(ko > total("이거 왜 안 돼?") + 300.0, "got {ko:.1}");
    }
}
