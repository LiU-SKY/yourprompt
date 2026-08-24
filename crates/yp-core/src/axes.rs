use std::collections::BTreeMap;

use serde::Serialize;
use yp_lang::{CueId, Hit, Lexicon, SmellId};

use crate::params::{actionability as b, clarity as c, context as d, decay, saturate, substance};
use crate::stats::PromptStats;

/// One scored component within an axis.
#[derive(Debug, Clone, Serialize)]
pub struct Component {
    pub id: &'static str,
    pub earned: f64,
    pub max: f64,
    /// Why it scored what it scored, for the `/score` report. Every number in
    /// the model is meant to be auditable rather than a black box.
    pub detail: String,
}

impl Component {
    pub(crate) fn new(id: &'static str, earned: f64, max: f64, detail: impl Into<String>) -> Self {
        Self {
            id,
            earned,
            max,
            detail: detail.into(),
        }
    }
}

/// One of the four scoring axes.
#[derive(Debug, Clone, Serialize)]
pub struct Axis {
    pub id: &'static str,
    pub earned: f64,
    pub max: f64,
    pub components: Vec<Component>,
}

impl Axis {
    pub(crate) fn from_components(id: &'static str, max: f64, components: Vec<Component>) -> Self {
        let earned = components.iter().map(|c| c.earned).sum();
        Self {
            id,
            earned,
            max,
            components,
        }
    }
}

/// Distinct matched terms per category.
///
/// Distinct rather than total on purpose: repeating "return value" three times
/// is not three times the evidence that a prompt specifies its output, and
/// counting it that way would reward padding.
pub fn distinct_by_category<C: Copy + Ord>(hits: &[Hit<C>]) -> BTreeMap<C, Vec<String>> {
    let mut out: BTreeMap<C, Vec<String>> = BTreeMap::new();
    for hit in hits {
        let bucket = out.entry(hit.id).or_default();
        let lowered = hit.term.to_lowercase();
        if !bucket.iter().any(|t| t.to_lowercase() == lowered) {
            bucket.push(hit.term.clone());
        }
    }
    out
}

/// Total occurrences per category.
///
/// Total rather than distinct here, because saying "it" four times really is
/// four unresolved references, not one.
pub fn total_by_category<C: Copy + Ord>(hits: &[Hit<C>]) -> BTreeMap<C, usize> {
    let mut out: BTreeMap<C, usize> = BTreeMap::new();
    for hit in hits {
        *out.entry(hit.id).or_default() += 1;
    }
    out
}

fn join_terms(terms: &[String], limit: usize) -> String {
    let shown: Vec<&str> = terms.iter().take(limit).map(String::as_str).collect();
    if terms.len() > limit {
        format!("{}, +{} more", shown.join(", "), terms.len() - limit)
    } else {
        shown.join(", ")
    }
}

/// Axis B -- does the prompt name one concrete action, with a stated result
/// and a way to tell when it is done?
pub fn actionability(cues: &BTreeMap<CueId, Vec<String>>) -> Axis {
    let empty = Vec::new();
    let actions = cues.get(&CueId::ActionVerb).unwrap_or(&empty);
    let conjunctions = cues.get(&CueId::Conjunction).unwrap_or(&empty);
    let io = cues.get(&CueId::IoSpec).unwrap_or(&empty);
    let acceptance = cues.get(&CueId::Acceptance).unwrap_or(&empty);

    let objective = if actions.is_empty() {
        Component::new(
            "objective",
            0.0,
            b::OBJECTIVE_MAX,
            "no concrete action verb -- the prompt does not say what to do",
        )
    } else {
        Component::new(
            "objective",
            b::OBJECTIVE_MAX,
            b::OBJECTIVE_MAX,
            format!("action: {}", join_terms(actions, 3)),
        )
    };

    // Every action verb past the first, plus every joining conjunction, is
    // another thing being asked for in one breath.
    //
    // Singularity is a property *of* an objective, so a prompt that names no
    // action scores zero here rather than full marks. Otherwise saying
    // nothing at all would be rewarded as admirably focused.
    let extra = actions.len().saturating_sub(1) + conjunctions.len();
    let singularity = if actions.is_empty() {
        Component::new(
            "singularity",
            0.0,
            b::SINGULARITY_MAX,
            "no objective to be singular about",
        )
    } else {
        Component::new(
            "singularity",
            decay(extra, b::SINGULARITY_MAX, b::SINGULARITY_DECAY),
            b::SINGULARITY_MAX,
            if extra == 0 {
                "one objective".to_string()
            } else {
                format!("{extra} additional objective(s) in one prompt")
            },
        )
    };

    let io_spec = Component::new(
        "io_spec",
        saturate(io.len(), b::IO_SPEC_MAX, b::IO_SPEC_RATE),
        b::IO_SPEC_MAX,
        if io.is_empty() {
            "result shape unspecified".to_string()
        } else {
            join_terms(io, 4)
        },
    );

    let acceptance = Component::new(
        "acceptance",
        saturate(acceptance.len(), b::ACCEPTANCE_MAX, b::ACCEPTANCE_RATE),
        b::ACCEPTANCE_MAX,
        if acceptance.is_empty() {
            "no stated way to tell when this is done".to_string()
        } else {
            join_terms(acceptance, 4)
        },
    );

    Axis::from_components(
        "actionability",
        crate::params::axis_max::ACTIONABILITY,
        vec![objective, singularity, io_spec, acceptance],
    )
}

/// Axis C -- how densely the prompt is packed with ambiguity smells.
///
/// Density, not count: three vague words in a four-word prompt is a different
/// problem from three in a two-hundred-word one. Each category is capped
/// first, so no single kind of vagueness can sink the axis on its own.
pub fn clarity(
    totals: &BTreeMap<SmellId, usize>,
    lexicon: &Lexicon,
    stats: &PromptStats,
    waived: &[SmellId],
) -> Axis {
    let mut weighted = 0.0;
    let mut worst: Vec<(SmellId, usize, f64)> = Vec::new();

    for (&id, &count) in totals {
        // A category the grounding axis judged more precisely is not charged
        // for twice. Vague pronouns are the case that matters: axis A asks
        // whether an "it" actually has an antecedent, which is a better
        // question than whether the word appears at all.
        if waived.contains(&id) {
            continue;
        }
        let Some(category) = lexicon.category(id) else {
            continue;
        };
        let counted = count.min(category.cap);
        let contribution = category.weight * counted as f64;
        weighted += contribution;
        worst.push((id, count, contribution));
    }

    let units = (stats.content_tokens as f64).max(c::MIN_CONTENT_UNITS);
    let density = weighted / units;
    let k = std::f64::consts::LN_2 / c::HALF_LIFE_DENSITY;
    // Scaled by substance *and* legibility: a prompt with nothing in it has no
    // smells, and neither does a mashed keyboard. Neither may collect full
    // marks for the absence.
    let earned = crate::params::axis_max::CLARITY
        * (-k * density).exp()
        * substance(stats.content_tokens)
        * stats.legibility();

    worst.sort_by(|a, b| b.2.total_cmp(&a.2));
    let detail = if worst.is_empty() && stats.content_tokens < c::MIN_CONTENT_UNITS as usize {
        format!(
            "too thin to judge ({} content tokens)",
            stats.content_tokens
        )
    } else if worst.is_empty() {
        "no ambiguity smells".to_string()
    } else {
        let listed: Vec<String> = worst
            .iter()
            .take(3)
            .map(|(id, count, _)| format!("{} x{}", id.label(), count))
            .collect();
        format!(
            "density {:.3} per content token ({})",
            density,
            listed.join(", ")
        )
    };

    Axis::from_components(
        "clarity",
        crate::params::axis_max::CLARITY,
        vec![Component::new(
            "smell_density",
            earned,
            crate::params::axis_max::CLARITY,
            detail,
        )],
    )
}

/// Gaussian in log-space: prompts that are too short are underspecified,
/// prompts that are too long dilute the instruction.
fn length_score(content_tokens: usize) -> f64 {
    if content_tokens == 0 {
        return 0.0;
    }
    let z = ((content_tokens as f64).ln() - d::IDEAL_TOKENS.ln()) / d::LENGTH_SIGMA;
    d::LENGTH_MAX * (-0.5 * z * z).exp()
}

fn variety_score(stats: &PromptStats) -> f64 {
    if stats.content_tokens < d::VARIETY_MIN_TOKENS {
        // Root TTR is dominated by noise on short text, so measuring it would
        // be worse than not measuring it. The fallback is still scaled by
        // substance, so an empty prompt earns nothing rather than a free 80%.
        return d::VARIETY_MAX
            * d::VARIETY_SHORT_TEXT_FRACTION
            * substance(stats.content_tokens)
            * stats.legibility();
    }
    let ratio = (stats.root_ttr() / d::FULL_VARIETY_RTTR).clamp(0.0, 1.0);
    // Mashing is maximally "varied" -- every keystroke a new token -- so
    // variety is credited only to text that reads as language.
    d::VARIETY_MAX * ratio * stats.legibility()
}

/// Axis D -- does the prompt bound its own scope, show what it means, and
/// come in a shape that is neither too thin nor too diluted?
pub fn context(cues: &BTreeMap<CueId, Vec<String>>, stats: &PromptStats) -> Axis {
    let empty = Vec::new();
    let scope_terms = cues.get(&CueId::ScopeConstraint).unwrap_or(&empty);
    let example_terms = cues.get(&CueId::ExampleMarker).unwrap_or(&empty);

    let scope = Component::new(
        "scope",
        saturate(scope_terms.len(), d::SCOPE_MAX, d::SCOPE_RATE),
        d::SCOPE_MAX,
        if scope_terms.is_empty() {
            "no stated boundary on what may change".to_string()
        } else {
            join_terms(scope_terms, 4)
        },
    );

    let code_points = if stats.has_code { d::CODE_PRESENT } else { 0.0 };
    // "for example" with nothing concrete anywhere in the prompt is a promise
    // of an example, not an example.
    let marker_backing = if stats.has_concrete {
        1.0
    } else {
        d::UNBACKED_MARKER_FRACTION
    };
    let marker_points = saturate(
        example_terms.len(),
        d::EXAMPLE_MARKER_MAX,
        d::EXAMPLE_MARKER_RATE,
    ) * marker_backing;
    let list_points = saturate(
        stats.list_lines,
        d::LIST_STRUCTURE_MAX,
        d::LIST_STRUCTURE_RATE,
    );
    let evidence = Component::new(
        "evidence",
        code_points + marker_points + list_points,
        d::EXAMPLE_MAX,
        format!(
            "code {}, {} example marker(s), {} list line(s)",
            if stats.has_code { "yes" } else { "no" },
            example_terms.len(),
            stats.list_lines
        ),
    );

    let shape = Component::new(
        "shape",
        length_score(stats.content_tokens) + variety_score(stats),
        d::SHAPE_MAX,
        format!(
            "{} content tokens, root TTR {:.2}",
            stats.content_tokens,
            stats.root_ttr()
        ),
    );

    Axis::from_components(
        "context",
        crate::params::axis_max::CONTEXT,
        vec![scope, evidence, shape],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::axis_max;
    use yp_lang::tokenize;

    fn resources() -> &'static yp_lang::Resources {
        yp_lang::resources().unwrap()
    }

    fn score_axes(text: &str) -> (Axis, Axis, Axis) {
        let r = resources();
        let tokens = tokenize(text);
        let stats = crate::stats::analyze(text, &tokens);
        let cues = distinct_by_category(&r.cue_matcher.find(text));
        let smells = total_by_category(&r.smells.find(text));
        (
            actionability(&cues),
            clarity(&smells, &r.lexicon, &stats, &[]),
            context(&cues, &stats),
        )
    }

    #[test]
    fn no_action_verb_earns_no_objective_points() {
        let (b, _, _) = score_axes("the login thing");
        let objective = b.components.iter().find(|c| c.id == "objective").unwrap();
        assert_eq!(objective.earned, 0.0);
    }

    #[test]
    fn a_single_clear_action_earns_full_objective_and_singularity() {
        let (b, _, _) = score_axes("refactor parse_args");
        let objective = b.components.iter().find(|c| c.id == "objective").unwrap();
        let singularity = b.components.iter().find(|c| c.id == "singularity").unwrap();
        assert_eq!(objective.earned, super::b::OBJECTIVE_MAX);
        assert_eq!(singularity.earned, super::b::SINGULARITY_MAX);
    }

    #[test]
    fn piling_on_objectives_costs_singularity_points() {
        let one = score_axes("refactor parse_args").0;
        let many = score_axes(
            "refactor parse_args and also add logging, plus deploy it, oh and rename it",
        )
        .0;
        let get = |a: &Axis| {
            a.components
                .iter()
                .find(|c| c.id == "singularity")
                .unwrap()
                .earned
        };
        assert!(get(&many) < get(&one), "{} vs {}", get(&many), get(&one));
    }

    #[test]
    fn vague_short_prompts_lose_most_clarity_points() {
        let (_, clear, _) = score_axes("rename parse_args to parse_arguments in src/cli.rs");
        let (_, vague, _) = score_axes("그냥 그거 좀 적당히 알아서 고쳐줘");
        assert!(vague.earned < clear.earned / 2.0, "{vague:?} vs {clear:?}");
    }

    #[test]
    fn clarity_forgives_a_smell_in_a_long_prompt() {
        // The same single smell should barely register when it is surrounded
        // by substance, but dominate when it is most of the prompt.
        let long = "refactor the token verifier in src/auth/login.rs so that it \
                    returns a Result instead of panicking on an expired token, \
                    update the three call sites in src/api/, keep the public \
                    signature stable, and make sure the existing tests still \
                    pass. it should be straightforward.";
        let short = "make it straightforward";
        let (_, long_axis, _) = score_axes(long);
        let (_, short_axis, _) = score_axes(short);
        assert!(
            long_axis.earned > short_axis.earned * 2.0,
            "{} vs {}",
            long_axis.earned,
            short_axis.earned
        );
    }

    #[test]
    fn axis_earnings_never_exceed_their_maxima() {
        let texts = [
            "",
            "fix it",
            "refactor and add and remove and deploy and rename and migrate",
            "그냥 알아서 잘 좀 해줘",
            &"add tests ".repeat(200),
        ];
        for text in texts {
            let (b, c, d) = score_axes(text);
            for axis in [&b, &c, &d] {
                assert!(
                    axis.earned <= axis.max + 1e-9,
                    "{} earned {} > max {} on {text:?}",
                    axis.id,
                    axis.earned,
                    axis.max
                );
                assert!(axis.earned >= 0.0, "{} went negative", axis.id);
            }
            assert_eq!(b.max, axis_max::ACTIONABILITY);
            assert_eq!(c.max, axis_max::CLARITY);
            assert_eq!(d.max, axis_max::CONTEXT);
        }
    }

    #[test]
    fn length_curve_peaks_at_the_ideal_and_falls_off_both_sides() {
        let ideal = length_score(d::IDEAL_TOKENS as usize);
        assert!(length_score(4) < ideal);
        assert!(length_score(600) < ideal);
        assert!((ideal - d::LENGTH_MAX).abs() < 1e-9);
    }

    #[test]
    fn scope_constraints_and_examples_earn_context_points() {
        let bare = score_axes("update the config loader").2;
        let bounded =
            score_axes("update the config loader, only in src/config.rs, don't touch the schema. for example: `load(\"a.toml\")`").2;
        assert!(
            bounded.earned > bare.earned,
            "{} vs {}",
            bounded.earned,
            bare.earned
        );
    }

    #[test]
    fn distinct_counting_ignores_repeats_but_totals_do_not() {
        let r = resources();
        let text = "it it it it";
        let hits = r.smells.find(text);
        let distinct = distinct_by_category(&hits);
        let totals = total_by_category(&hits);
        assert_eq!(distinct[&SmellId::VaguePronoun].len(), 1);
        assert_eq!(totals[&SmellId::VaguePronoun], 4);
    }
}
