//! Every tunable constant in the scoring model, in one place.
//!
//! These are the numbers that milestone M5 calibrates against the published
//! ambiguity benchmarks. Keeping them together means calibration touches one
//! file and every change is visible in one diff, rather than being scattered
//! through the axis code as magic numbers.
//!
//! The current values are reasoned defaults, not fitted ones. They have not
//! yet been validated against HumanEvalComm / ClarEval / Ambig-SWE -- until
//! `yp bench` exists, treat the absolute numbers as provisional and only the
//! *ordering* they produce as meaningful.

/// Axis maxima. They sum to 1000.
pub mod axis_max {
    /// A -- grounding and referent resolution. Requires a repository index;
    /// when none is available this axis is dropped and the rest are
    /// renormalised to 1000.
    pub const GROUNDING: f64 = 350.0;
    /// B -- actionability and objective.
    pub const ACTIONABILITY: f64 = 250.0;
    /// C -- freedom from ambiguity smells.
    pub const CLARITY: f64 = 250.0;
    /// D -- context sufficiency and form.
    pub const CONTEXT: f64 = 150.0;

    pub const TOTAL: f64 = GROUNDING + ACTIONABILITY + CLARITY + CONTEXT;
    /// What B + C + D alone add up to, before renormalisation.
    pub const WITHOUT_GROUNDING: f64 = ACTIONABILITY + CLARITY + CONTEXT;
}

/// B -- actionability and objective.
pub mod actionability {
    /// Does the prompt name a concrete action at all?
    pub const OBJECTIVE_MAX: f64 = 60.0;
    /// Is it *one* action rather than a pile of them? (ISO 29148 singularity.)
    pub const SINGULARITY_MAX: f64 = 40.0;
    /// Does it say what the result should look like?
    pub const IO_SPEC_MAX: f64 = 80.0;
    /// Does it say how "done" will be judged?
    pub const ACCEPTANCE_MAX: f64 = 70.0;

    /// Decay per extra objective. Two objectives keep ~74% of the singularity
    /// points, three ~55%: joining related work with "and" is normal and
    /// should cost something, but not be treated as a defect.
    pub const SINGULARITY_DECAY: f64 = 0.30;
    /// Saturation rate for I/O specification cues.
    pub const IO_SPEC_RATE: f64 = 0.80;
    /// Saturation rate for acceptance-criteria cues.
    pub const ACCEPTANCE_RATE: f64 = 0.90;
}

/// A -- grounding and referent resolution.
pub mod grounding {
    /// Can the things this prompt names be pinned down in this repository?
    pub const RESOLUTION_MAX: f64 = 150.0;
    /// How specific are its words relative to this repository's vocabulary?
    pub const SPECIFICITY_MAX: f64 = 120.0;
    /// Does it point at anything, or only gesture?
    pub const DEIXIS_MAX: f64 = 80.0;

    /// A word appearing in more than this fraction of files is treated as
    /// prose rather than as a name. "the" and "return" are in nearly every
    /// file; taking them as referents would drown the real ones.
    pub const UBIQUITY_CUTOFF: f64 = 0.5;

    /// Explicit names -- backticked code, paths, snake_case, camelCase --
    /// count for full weight. A bare prose word that happens to exist in the
    /// repository counts for less, because the user may not have meant it as
    /// a name at all.
    pub const EXPLICIT_WEIGHT: f64 = 1.0;
    pub const PROSE_WEIGHT: f64 = 0.5;

    /// Simplified Clarity Score at which specificity is worth half its
    /// points. SCS is a Kullback-Leibler divergence between the prompt's
    /// term distribution and the repository's, following the pre-retrieval
    /// query-performance-prediction literature (Hauff et al., CIKM 2008).
    pub const SCS_HALF_LIFE: f64 = 4.0;

    /// Weight given to an unseen term when estimating its collection
    /// probability, so a term absent from the repository does not make the
    /// divergence infinite.
    pub const UNSEEN_TERM_WEIGHT: f64 = 0.5;

    /// Decay per dangling deictic -- an "it" or "그거" with nothing to
    /// attach to.
    pub const DEIXIS_DECAY: f64 = 0.6;
}

/// C -- freedom from ambiguity smells.
pub mod clarity {
    /// Smell density is measured per content token, but the denominator never
    /// drops below this. Without a floor, a four-word prompt with one smell
    /// would score the same as a forty-word prompt with ten, and terse vague
    /// prompts are exactly what this axis exists to catch.
    pub const MIN_CONTENT_UNITS: f64 = 8.0;

    /// Weighted smell density at which the axis loses half its points.
    /// Roughly: one mid-weight smell every five content words.
    pub const HALF_LIFE_DENSITY: f64 = 0.20;
}

/// D -- context sufficiency and form.
pub mod context {
    pub const SCOPE_MAX: f64 = 60.0;
    pub const EXAMPLE_MAX: f64 = 50.0;
    pub const SHAPE_MAX: f64 = 40.0;

    /// Saturation rate for scope-constraint cues.
    pub const SCOPE_RATE: f64 = 0.70;

    /// How `EXAMPLE_MAX` splits between pasted code, explicit example
    /// markers, and list structure.
    pub const CODE_PRESENT: f64 = 20.0;
    pub const EXAMPLE_MARKER_MAX: f64 = 15.0;
    pub const LIST_STRUCTURE_MAX: f64 = 15.0;
    pub const EXAMPLE_MARKER_RATE: f64 = 1.0;
    pub const LIST_STRUCTURE_RATE: f64 = 0.5;

    /// How `SHAPE_MAX` splits between length and lexical variety.
    pub const LENGTH_MAX: f64 = 25.0;
    pub const VARIETY_MAX: f64 = 15.0;

    /// The length curve is a Gaussian in log space: too short is
    /// underspecified, too long dilutes the instruction. Centre and width are
    /// in natural log of content-token count.
    pub const IDEAL_TOKENS: f64 = 45.0;
    pub const LENGTH_SIGMA: f64 = 0.90;

    /// Root type-token ratio (types / sqrt(tokens)) that earns full variety
    /// points. Ordinary prose sits near this; heavily repetitive text falls
    /// well below it.
    pub const FULL_VARIETY_RTTR: f64 = 6.0;
    /// Below this many content tokens, root TTR is too noisy to mean
    /// anything, so variety is scored at this fraction of the maximum
    /// instead of being measured.
    pub const VARIETY_MIN_TOKENS: usize = 20;
    pub const VARIETY_SHORT_TEXT_FRACTION: f64 = 0.8;
}

/// How much of a prompt there is to judge at all.
///
/// Runs from 0 to 1, reaching 1 at `clarity::MIN_CONTENT_UNITS` content
/// tokens. Axes that measure the *absence* of a defect are multiplied by it,
/// because absence of evidence is not evidence of quality: an empty prompt
/// contains no ambiguity smells, and without this it would score full marks
/// for clarity.
pub fn substance(content_tokens: usize) -> f64 {
    (content_tokens as f64 / clarity::MIN_CONTENT_UNITS).min(1.0)
}

/// A saturating curve: `max * (1 - e^(-rate * n))`.
///
/// Used wherever more evidence should keep helping but with diminishing
/// returns. Approaches `max` asymptotically and never exceeds it, so no
/// amount of keyword stuffing can push a component past its ceiling.
pub fn saturate(n: usize, max: f64, rate: f64) -> f64 {
    if n == 0 {
        return 0.0;
    }
    max * (1.0 - (-rate * n as f64).exp())
}

/// An exponential decay: `max * e^(-rate * n)`.
pub fn decay(n: usize, max: f64, rate: f64) -> f64 {
    max * (-rate * n as f64).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_maxima_sum_to_one_thousand() {
        assert_eq!(axis_max::TOTAL, 1000.0);
        assert_eq!(axis_max::WITHOUT_GROUNDING, 650.0);
    }

    #[test]
    fn grounding_components_fill_their_axis() {
        let sum = grounding::RESOLUTION_MAX + grounding::SPECIFICITY_MAX + grounding::DEIXIS_MAX;
        assert_eq!(sum, axis_max::GROUNDING);
    }

    #[test]
    fn actionability_components_fill_their_axis() {
        let sum = actionability::OBJECTIVE_MAX
            + actionability::SINGULARITY_MAX
            + actionability::IO_SPEC_MAX
            + actionability::ACCEPTANCE_MAX;
        assert_eq!(sum, axis_max::ACTIONABILITY);
    }

    #[test]
    fn context_components_fill_their_axis() {
        let sum = context::SCOPE_MAX + context::EXAMPLE_MAX + context::SHAPE_MAX;
        assert_eq!(sum, axis_max::CONTEXT);
        assert_eq!(
            context::CODE_PRESENT + context::EXAMPLE_MARKER_MAX + context::LIST_STRUCTURE_MAX,
            context::EXAMPLE_MAX
        );
        assert_eq!(
            context::LENGTH_MAX + context::VARIETY_MAX,
            context::SHAPE_MAX
        );
    }

    #[test]
    fn saturate_is_monotonic_and_bounded() {
        let mut prev = 0.0;
        for n in 0..50 {
            let v = saturate(n, 80.0, 0.8);
            assert!(v >= prev, "not monotonic at {n}");
            assert!(v <= 80.0, "exceeded max at {n}");
            prev = v;
        }
        assert_eq!(saturate(0, 80.0, 0.8), 0.0);
        // Approaches the ceiling but is still climbing where it matters:
        // a handful of cues must not be worth the same as a hundred.
        assert!(saturate(1, 80.0, 0.8) < saturate(4, 80.0, 0.8));
        assert!(saturate(4, 80.0, 0.8) < 80.0);
    }

    #[test]
    fn substance_gates_on_having_said_anything() {
        assert_eq!(substance(0), 0.0);
        assert!(substance(4) > 0.0 && substance(4) < 1.0);
        assert_eq!(substance(8), 1.0);
        assert_eq!(substance(500), 1.0);
    }

    #[test]
    fn decay_is_monotonic_and_starts_at_max() {
        assert_eq!(decay(0, 40.0, 0.3), 40.0);
        let mut prev = f64::MAX;
        for n in 0..50 {
            let v = decay(n, 40.0, 0.3);
            assert!(v <= prev, "not monotonic at {n}");
            assert!(v >= 0.0);
            prev = v;
        }
    }
}
