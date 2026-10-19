//! HumanEvalComm: does the score rank a specification above a damaged
//! version of itself?
//!
//! Every prompt scorer on GitHub asserts that its number reflects prompt
//! quality. None of them checks. This does.
//!
//! The method is the only honest one available: take prompts that have been
//! *deliberately damaged* in known ways by someone else, and ask whether the
//! score ranks the original above the damaged version. HumanEvalComm (Wu et
//! al.) supplies exactly that -- 164 problems, each perturbed into ambiguous,
//! inconsistent and incomplete variants, plus combinations, giving both a
//! pairwise ordering to check and a defect count to correlate against.
//!
//! # What this does and does not show
//!
//! HumanEvalComm's prompts are function stubs with docstrings, not the
//! imperative requests a coding agent actually receives. The distribution is
//! not the one this tool was built for, and the absolute scores it produces
//! here mean little. What *is* meaningful is the ordering: if injecting
//! ambiguity into a specification does not lower the score, the score is not
//! measuring ambiguity. Reporting it any other way would be the same
//! overclaiming this whole exercise exists to avoid.
//!
//! Grounding is inactive throughout, since these prompts have no repository.
//! Ablation therefore covers axes B, C and D.
//!
//! # The length baseline
//!
//! The damage changes how long a prompt is. An incomplete variant is its
//! original with text cut out -- shorter in all 164 pairs, by 247 characters
//! on average -- while an ambiguous one is longer in 107 of 164. So "the
//! longer prompt is the original" is right more often than not without
//! reading a word: when this baseline was added it got 72.7% of the
//! lexically reachable pairs and 59.0% of all of them, against the score's
//! 65.9% and 53.3%. Every ordering figure is printed beside it, counted by
//! the same rule, because a score has to be read against that trivial
//! predictor rather than against a coin toss.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use yp_core::Score;

use super::{spearman, total_without, Tally};
use crate::session;

const HUMANEVALCOMM_URL: &str = "https://raw.githubusercontent.com/jie-jw-wu/human-eval-comm/main/Benchmark/HumanEvalComm_v2.jsonl";

/// The defect categories HumanEvalComm injects, and the field holding each
/// perturbed variant.
///
/// The suffix letters are the paper's: `a` ambiguity, `c` inconsistency,
/// `p` incompleteness. A field with two letters carries both defects.
const VARIANTS: &[Variant] = &[
    Variant::new("prompt1a", "ambiguity", 1, true),
    Variant::new("prompt1c", "inconsistency", 1, false),
    Variant::new("prompt1p", "incompleteness", 1, true),
    Variant::new("prompt2ac", "ambiguity+inconsistency", 2, false),
    Variant::new("prompt2ap", "ambiguity+incompleteness", 2, true),
    Variant::new("prompt2cp", "inconsistency+incompleteness", 2, false),
    Variant::new("prompt3acp", "all three", 3, false),
];

struct Variant {
    field: &'static str,
    category: &'static str,
    defects: usize,
    /// Whether the injected defect is in principle visible without evaluating
    /// the program. See the note in the report for why inconsistency is not.
    lexically_reachable: bool,
}

impl Variant {
    const fn new(
        field: &'static str,
        category: &'static str,
        defects: usize,
        lexically_reachable: bool,
    ) -> Self {
        Variant {
            field,
            category,
            defects,
            lexically_reachable,
        }
    }
}

/// One original-versus-damaged comparison.
struct Pair {
    category: &'static str,
    defects: usize,
    lexically_reachable: bool,
    original: Score,
    perturbed: Score,
    /// Lengths in characters, which is all the length baseline reads.
    original_chars: usize,
    perturbed_chars: usize,
}

impl Pair {
    fn new(
        variant: &Variant,
        original_text: &str,
        original: &Score,
        perturbed_text: &str,
        perturbed: Score,
    ) -> Self {
        Pair {
            category: variant.category,
            defects: variant.defects,
            lexically_reachable: variant.lexically_reachable,
            original: original.clone(),
            perturbed,
            original_chars: original_text.chars().count(),
            perturbed_chars: perturbed_text.chars().count(),
        }
    }
}

/// One row of the ordering tables: the score and the length baseline,
/// tallied over the same pairs.
#[derive(Default)]
struct Row {
    score: Tally,
    length: Tally,
}

impl Row {
    fn add(&mut self, pair: &Pair) {
        self.score.add(pair.original.total, pair.perturbed.total);
        self.length
            .add(pair.original_chars as f64, pair.perturbed_chars as f64);
    }

    /// The columns both ordering tables share, after their labels.
    fn cells(&self) -> String {
        format!(
            "{} | {:.1}% | {} | {:.1} | {:.1}% ({})",
            self.score.total,
            self.score.accuracy() * 100.0,
            self.score.ties,
            self.score.mean_delta(),
            self.length.accuracy() * 100.0,
            self.length.ties,
        )
    }
}

fn cache_path() -> Option<PathBuf> {
    session::state_dir().map(|d| d.join("bench").join("humanevalcomm.jsonl"))
}

/// Fetch the dataset if it is not already cached.
///
/// Shelling out to curl rather than linking an HTTP stack: this is a
/// development command, and the alternative is putting rustls into a binary
/// that runs on every keystroke.
fn ensure_dataset(refresh: bool) -> Result<PathBuf, String> {
    let path = cache_path().ok_or("no state directory available")?;
    if path.exists() && !refresh {
        return Ok(path);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    eprintln!("fetching HumanEvalComm...");
    let tmp = path.with_extension("part");
    let status = Command::new("curl")
        .args(["-fsSL", HUMANEVALCOMM_URL, "-o"])
        .arg(&tmp)
        .status()
        .map_err(|e| format!("could not run curl: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("curl failed downloading {HUMANEVALCOMM_URL}"));
    }
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Read the dataset and score every original-versus-perturbed pair.
fn load_pairs(path: &Path, limit: Option<usize>) -> Result<Vec<Pair>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut pairs = Vec::new();

    for (line_no, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        if limit.is_some_and(|n| line_no >= n) {
            break;
        }
        let record: serde_json::Value =
            serde_json::from_str(line).map_err(|e| format!("line {}: {e}", line_no + 1))?;
        let Some(original_text) = record.get("prompt").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(original) = yp_core::score(original_text) else {
            return Err("language resources unavailable".into());
        };

        for variant in VARIANTS {
            let Some(text) = record.get(variant.field).and_then(|v| v.as_str()) else {
                continue;
            };
            if text.trim().is_empty() {
                continue;
            }
            let Some(perturbed) = yp_core::score(text) else {
                continue;
            };
            pairs.push(Pair::new(
                variant,
                original_text,
                &original,
                text,
                perturbed,
            ));
        }
    }
    Ok(pairs)
}

fn render_report(pairs: &[Pair], ablation: bool) -> String {
    let mut out = String::new();
    out.push_str("# yourprompt benchmark\n\n");
    out.push_str(
        "Dataset: [HumanEvalComm](https://github.com/jie-jw-wu/human-eval-comm) \
         (Wu et al.), 164 HumanEval problems perturbed into ambiguous, \
         inconsistent and incomplete variants.\n\n\
         The question asked is whether the score ranks an original \
         specification above a deliberately damaged version of itself. \
         Absolute values are not meaningful here: these prompts are function \
         stubs with docstrings, not the imperative requests this tool is built \
         for. Grounding is inactive throughout, as the prompts have no \
         repository.\n\n",
    );

    // ---- overall + per category -----------------------------------------
    let mut overall = Row::default();
    let mut reachable = Row::default();
    let mut by_category: BTreeMap<&str, Row> = BTreeMap::new();
    for pair in pairs {
        overall.add(pair);
        if pair.lexically_reachable {
            reachable.add(pair);
        }
        by_category.entry(pair.category).or_default().add(pair);
    }

    out.push_str(&format!(
        "## Pairwise ordering\n\n\
         Each row gives how often the score ranks the original above its \
         damaged version, and beside it the length baseline: \"longer wins\" \
         calls whichever prompt has more characters the original and reads \
         nothing else. Both count a tie as a miss. Read the score against the \
         baseline, not against 50%.\n\n\
         | | Pairs | Correct | Ties | Mean margin | Longer wins (ties) |\n\
         |---|---:|---:|---:|---:|---:|\n\
         | lexically reachable | {} |\n\
         | all | {} |\n\n\
         A defect is *lexically reachable* when it is visible without \
         evaluating the program. The gap between the two rows is not a \
         tuning problem; see the note below.\n\n",
        reachable.cells(),
        overall.cells(),
    ));

    out.push_str(
        "| Defect injected | Reachable | Pairs | Correct | Ties | Mean margin | Longer wins (ties) |\n",
    );
    out.push_str("|---|:---:|---:|---:|---:|---:|---:|\n");
    for (category, row) in &by_category {
        let is_reachable = pairs
            .iter()
            .find(|p| p.category == *category)
            .is_some_and(|p| p.lexically_reachable);
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            category,
            if is_reachable { "yes" } else { "no" },
            row.cells(),
        ));
    }

    out.push_str(
        "\n### Why inconsistency is out of reach\n\n\
         HumanEvalComm creates an inconsistent problem by making a docstring's \
         worked example contradict its prose -- `truncate_number(3.5) -> 3` \
         where the text says \"return the decimal part\". Detecting that means \
         knowing the decimal part of 3.5 is 0.5. No dictionary, density \
         measure or corpus statistic reaches it, which is why those pairs \
         mostly come out as exact ties: to a lexical scorer the two prompts \
         are indistinguishable. That is a boundary of the deterministic, \
         LLM-free approach rather than a defect in this implementation of it, \
         and it is reported as a boundary instead of averaged away.\n",
    );

    // ---- severity ordering ----------------------------------------------
    let severity: Vec<(f64, f64)> = pairs
        .iter()
        .map(|p| (p.defects as f64, p.perturbed.total))
        .chain(pairs.iter().map(|p| (0.0, p.original.total)))
        .collect();
    out.push_str(&format!(
        "\n## Severity ordering\n\n\
         Spearman correlation between number of injected defects and score: \
         **{:.3}** (negative is correct -- more damage, lower score), \
         over {} observations.\n",
        spearman(&severity),
        severity.len(),
    ));

    // ---- ablation --------------------------------------------------------
    if ablation {
        out.push_str(
            "\n## Ablation\n\n\
             Each axis removed in turn, with the remainder renormalised. \
             The drop in accuracy is that axis's contribution.\n\n\
             | Axis removed | Pairwise accuracy | Change |\n|---|---:|---:|\n",
        );
        out.push_str(&format!(
            "| none (full model) | {:.1}% | -- |\n",
            overall.score.accuracy() * 100.0
        ));
        for axis in ["actionability", "clarity", "context"] {
            let mut tally = Tally::default();
            for pair in pairs {
                tally.add(
                    total_without(&pair.original, axis),
                    total_without(&pair.perturbed, axis),
                );
            }
            out.push_str(&format!(
                "| {} | {:.1}% | {:+.1} pp |\n",
                axis,
                tally.accuracy() * 100.0,
                (tally.accuracy() - overall.score.accuracy()) * 100.0,
            ));
        }
    }

    out
}

pub fn run(
    refresh: bool,
    ablation: bool,
    limit: Option<usize>,
    report: Option<String>,
) -> ExitCode {
    let path = match ensure_dataset(refresh) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("yp: {e}");
            return ExitCode::FAILURE;
        }
    };

    let pairs = match load_pairs(&path, limit) {
        Ok(pairs) => pairs,
        Err(e) => {
            eprintln!("yp: could not read the dataset: {e}");
            return ExitCode::FAILURE;
        }
    };
    if pairs.is_empty() {
        eprintln!("yp: no comparable pairs found in the dataset");
        return ExitCode::FAILURE;
    }

    let rendered = render_report(&pairs, ablation);
    match report {
        Some(path) => {
            if let Err(e) = std::fs::write(&path, &rendered) {
                eprintln!("yp: could not write {path}: {e}");
                return ExitCode::FAILURE;
            }
            println!("wrote {path}");
        }
        None => print!("{rendered}"),
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tally_counts_wins_ties_and_margin() {
        let mut t = Tally::default();
        t.add(700.0, 500.0);
        t.add(400.0, 600.0);
        t.add(500.0, 500.0);
        assert_eq!(t.total, 3);
        assert_eq!(t.correct, 1);
        assert_eq!(t.ties, 1);
        assert!((t.accuracy() - 1.0 / 3.0).abs() < 1e-9);
        assert!((t.mean_delta() - 0.0).abs() < 1e-9);
    }

    #[test]
    fn spearman_is_one_for_a_perfectly_increasing_relation() {
        let pairs: Vec<(f64, f64)> = (0..20).map(|i| (i as f64, i as f64 * 3.0)).collect();
        assert!((spearman(&pairs) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn spearman_is_minus_one_for_a_perfectly_decreasing_relation() {
        let pairs: Vec<(f64, f64)> = (0..20).map(|i| (i as f64, -(i as f64))).collect();
        assert!((spearman(&pairs) + 1.0).abs() < 1e-9);
    }

    #[test]
    fn spearman_handles_ties_without_dividing_by_zero() {
        let flat: Vec<(f64, f64)> = (0..10).map(|i| (i as f64, 5.0)).collect();
        assert_eq!(spearman(&flat), 0.0);
        assert_eq!(spearman(&[]), 0.0);
        assert_eq!(spearman(&[(1.0, 2.0)]), 0.0);
    }

    #[test]
    fn spearman_ranks_ties_by_their_average() {
        // Values [10, 20, 20, 30] rank as [1, 2.5, 2.5, 4]; paired with a
        // strictly increasing series that is still a strong positive
        // correlation, but not a perfect one.
        let pairs = [(10.0, 1.0), (20.0, 2.0), (20.0, 3.0), (30.0, 4.0)];
        let rho = spearman(&pairs);
        assert!(rho > 0.9 && rho < 1.0, "got {rho}");
    }

    #[test]
    fn ablation_renormalises_over_the_axes_that_remain() {
        let score = yp_core::score("refactor parse_args in src/cli.rs so it returns Config")
            .expect("scores");
        for axis in ["actionability", "clarity", "context"] {
            let total = total_without(&score, axis);
            assert!(
                (0.0..=1000.0).contains(&total),
                "{axis} ablation gave {total}"
            );
        }
        // Dropping an axis the prompt scored well on must lower the total,
        // not raise it through a renormalisation artefact.
        let full = score.total;
        let without_all = total_without(&score, "actionability")
            + total_without(&score, "clarity")
            + total_without(&score, "context");
        assert!(without_all > 0.0, "full {full}");
    }

    /// A pair built from text the way `load_pairs` builds one. The scores are
    /// real but beside the point: only the lengths are under test.
    fn pair(category: &str, original: &str, perturbed: &str) -> Pair {
        let variant = VARIANTS
            .iter()
            .find(|v| v.category == category)
            .expect("a HumanEvalComm category");
        let score = |text: &str| yp_core::score(text).expect("scores");
        Pair::new(
            variant,
            original,
            &score(original),
            perturbed,
            score(perturbed),
        )
    }

    // One specification damaged the three ways HumanEvalComm damages one:
    // text cut out, the example contradicted, a vague alternative written in.
    const ORIGINAL: &str = "Return the decimal part of 3.5, which is 0.5.";
    const CUT: &str = "Return the decimal part.";
    const CONTRADICTED: &str = "Return the decimal part of 3.5, which is 3.0.";
    const VAGUE: &str = "Return a certain part of 3.5 (e.g. integer or decimal).";

    #[test]
    fn the_length_baseline_calls_the_longer_prompt_the_original() {
        let mut row = Row::default();
        // The cut original is the longer side, so the baseline is right.
        row.add(&pair("incompleteness", ORIGINAL, CUT));
        assert_eq!((row.length.correct, row.length.ties), (1, 0));
        // The vague version is the longer side, so the baseline is wrong.
        row.add(&pair("ambiguity", ORIGINAL, VAGUE));
        assert_eq!((row.length.correct, row.length.ties), (1, 0));
        // The contradiction changes no length at all.
        row.add(&pair("inconsistency", ORIGINAL, CONTRADICTED));
        assert_eq!((row.length.correct, row.length.ties), (1, 1));
        // The tie is a miss, as it is for the score.
        assert!((row.length.accuracy() - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(row.score.total, row.length.total);
    }

    #[test]
    fn the_length_baseline_counts_characters_not_bytes() {
        // Three Hangul syllables are nine bytes but three characters, so the
        // six-character damaged side is the longer one.
        let mut row = Row::default();
        row.add(&pair("ambiguity", "반환해", "fix it"));
        assert_eq!((row.length.correct, row.length.ties), (0, 0));
    }

    #[test]
    fn every_ordering_row_carries_the_length_baseline() {
        let pairs = [
            pair("ambiguity", ORIGINAL, VAGUE),
            pair("ambiguity", ORIGINAL, "Return some part of 3.5."),
            pair("incompleteness", ORIGINAL, CUT),
            pair("inconsistency", ORIGINAL, CONTRADICTED),
        ];
        let report = render_report(&pairs, false);
        for (label, baseline) in [
            // Ambiguity and incompleteness are the reachable defects: three
            // pairs, two of them won on length alone.
            ("lexically reachable", "66.7% (0)"),
            ("all", "50.0% (1)"),
            ("ambiguity", "50.0% (0)"),
            ("incompleteness", "100.0% (0)"),
            ("inconsistency", "0.0% (1)"),
        ] {
            let row = report
                .lines()
                .find(|line| line.starts_with(&format!("| {label} |")))
                .unwrap_or_else(|| panic!("no {label} row in\n{report}"));
            assert!(row.ends_with(&format!("| {baseline} |")), "{row}");
        }
    }
}
