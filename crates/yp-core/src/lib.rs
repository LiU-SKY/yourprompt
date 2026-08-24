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

pub mod axes;
pub mod corpus;
pub mod grade;
pub mod grounding;
pub mod params;
pub mod prompt;
pub mod stats;

use serde::Serialize;

pub use axes::{Axis, Component};
pub use corpus::{Corpus, MapCorpus, TermFacts};
pub use stats::PromptStats;

/// A scored prompt.
#[derive(Debug, Clone, Serialize)]
pub struct Score {
    /// 0.0 to 1000.0.
    pub total: f64,
    pub grade: &'static str,
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
    let tokens = yp_lang::tokenize(parts.instruction());
    grounding::referents(&tokens, &[], corpus)
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
/// present: an objective, a result shape, an acceptance criterion, or a
/// scope boundary. Requirements smells presuppose a requirement.
pub(crate) fn request_factor(
    cues: &std::collections::BTreeMap<yp_lang::CueId, Vec<String>>,
) -> f64 {
    let requestish = [
        yp_lang::CueId::ActionVerb,
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
pub fn score_parts(parts: &prompt::Parts<'_>, corpus: Option<&dyn Corpus>) -> Option<Score> {
    let resources = yp_lang::resources()?;
    let text = parts.instruction();

    // One tokenisation and one code-region scan, shared by every axis.
    let tokens = yp_lang::tokenize(text);
    let stats = stats::analyze(text, &tokens);
    let code = yp_lang::token::code_regions(text);

    let cue_hits = resources.cue_matcher.find_masked(text, &code);
    let smell_hits = resources.smells.find_masked(text, &code);

    let cues = axes::distinct_by_category(&cue_hits);
    let smells = axes::total_by_category(&smell_hits);
    let request = request_factor(&cues);

    let grounding_axis = corpus.map(|corpus| {
        let offsets = grounding::pronoun_offsets(&smell_hits);
        // Spans already claimed by a cue or a smell are instruction words,
        // not names the user is pointing at.
        let instruction_words: Vec<yp_lang::Span> = cue_hits
            .iter()
            .map(|h| h.span)
            .chain(smell_hits.iter().map(|h| h.span))
            .collect();
        grounding::grounding(
            &tokens,
            &offsets,
            &instruction_words,
            &parts.attachments,
            corpus,
            stats.diversity() * request,
        )
        .0
    });

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

    Some(Score {
        total,
        grade: grade::grade(total),
        grounding: grounding_axis,
        actionability,
        clarity,
        context,
        renormalized,
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
    }

    #[test]
    fn grounding_is_absent_until_the_index_exists() {
        let s = score("fix src/main.rs").unwrap();
        assert!(s.grounding.is_none());
        assert!(s.renormalized);
    }
}
