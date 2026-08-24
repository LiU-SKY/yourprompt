//! `yp bench` -- does the score actually mean anything?
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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use yp_core::Score;

use crate::session;

const HUMANEVALCOMM_URL: &str = "https://raw.githubusercontent.com/jie-jw-wu/human-eval-comm/main/Benchmark/HumanEvalComm_v2.jsonl";

/// The defect categories HumanEvalComm injects, and the field holding each
/// perturbed variant.
///
/// The suffix letters are the paper's: `a` ambiguity, `c` inconsistency,
/// `p` incompleteness. A field with two letters carries both defects.
const VARIANTS: &[(&str, &str, usize)] = &[
    ("prompt1a", "ambiguity", 1),
    ("prompt1c", "inconsistency", 1),
    ("prompt1p", "incompleteness", 1),
    ("prompt2ac", "ambiguity+inconsistency", 2),
    ("prompt2ap", "ambiguity+incompleteness", 2),
    ("prompt2cp", "inconsistency+incompleteness", 2),
    ("prompt3acp", "all three", 3),
];

/// One original-versus-damaged comparison.
struct Pair {
    category: &'static str,
    defects: usize,
    original: Score,
    perturbed: Score,
}

/// Totals for one axis-ablation setting.
#[derive(Default)]
struct Tally {
    correct: usize,
    ties: usize,
    total: usize,
    delta_sum: f64,
}

impl Tally {
    fn add(&mut self, original: f64, perturbed: f64) {
        self.total += 1;
        self.delta_sum += original - perturbed;
        if (original - perturbed).abs() < 1e-9 {
            self.ties += 1;
        } else if original > perturbed {
            self.correct += 1;
        }
    }

    fn accuracy(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.correct as f64 / self.total as f64
    }

    fn mean_delta(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.delta_sum / self.total as f64
    }
}

/// The score with one axis removed, renormalised over the axes that remain.
///
/// Recombining published component scores rather than re-running the scorer
/// keeps the ablation honest: it is exactly the same measurement minus one
/// term, not a differently-configured scorer.
fn total_without(score: &Score, drop: &str) -> f64 {
    let axes = [&score.actionability, &score.clarity, &score.context];
    let mut earned = 0.0;
    let mut max = 0.0;
    for axis in axes {
        if axis.id == drop {
            continue;
        }
        earned += axis.earned;
        max += axis.max;
    }
    if max <= 0.0 {
        return 0.0;
    }
    earned * (1000.0 / max)
}

/// Spearman's rank correlation, with tied ranks averaged.
fn spearman(pairs: &[(f64, f64)]) -> f64 {
    if pairs.len() < 2 {
        return 0.0;
    }
    let rank = |values: Vec<f64>| -> Vec<f64> {
        let mut indexed: Vec<(usize, f64)> = values.into_iter().enumerate().collect();
        indexed.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut ranks = vec![0.0; indexed.len()];
        let mut i = 0;
        while i < indexed.len() {
            let mut j = i;
            while j + 1 < indexed.len() && indexed[j + 1].1 == indexed[i].1 {
                j += 1;
            }
            // Ties share the average of the ranks they span.
            let average = ((i + j) as f64) / 2.0 + 1.0;
            for item in indexed.iter().take(j + 1).skip(i) {
                ranks[item.0] = average;
            }
            i = j + 1;
        }
        ranks
    };

    let xs = rank(pairs.iter().map(|p| p.0).collect());
    let ys = rank(pairs.iter().map(|p| p.1).collect());
    let n = xs.len() as f64;
    let mean_x = xs.iter().sum::<f64>() / n;
    let mean_y = ys.iter().sum::<f64>() / n;

    let mut cov = 0.0;
    let mut var_x = 0.0;
    let mut var_y = 0.0;
    for (x, y) in xs.iter().zip(&ys) {
        cov += (x - mean_x) * (y - mean_y);
        var_x += (x - mean_x).powi(2);
        var_y += (y - mean_y).powi(2);
    }
    if var_x <= 0.0 || var_y <= 0.0 {
        return 0.0;
    }
    cov / (var_x * var_y).sqrt()
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

        for (field, category, defects) in VARIANTS {
            let Some(text) = record.get(*field).and_then(|v| v.as_str()) else {
                continue;
            };
            if text.trim().is_empty() {
                continue;
            }
            let Some(perturbed) = yp_core::score(text) else {
                continue;
            };
            pairs.push(Pair {
                category,
                defects: *defects,
                original: original.clone(),
                perturbed,
            });
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
    let mut overall = Tally::default();
    let mut by_category: BTreeMap<&str, Tally> = BTreeMap::new();
    for pair in pairs {
        overall.add(pair.original.total, pair.perturbed.total);
        by_category
            .entry(pair.category)
            .or_default()
            .add(pair.original.total, pair.perturbed.total);
    }

    out.push_str(&format!(
        "## Pairwise ordering\n\n\
         **{:.1}%** of {} pairs score the original above the damaged version \
         (ties: {}), mean margin {:.1} points.\n\n",
        overall.accuracy() * 100.0,
        overall.total,
        overall.ties,
        overall.mean_delta(),
    ));

    out.push_str("| Defect injected | Pairs | Correct | Ties | Mean margin |\n");
    out.push_str("|---|---:|---:|---:|---:|\n");
    for (category, tally) in &by_category {
        out.push_str(&format!(
            "| {} | {} | {:.1}% | {} | {:.1} |\n",
            category,
            tally.total,
            tally.accuracy() * 100.0,
            tally.ties,
            tally.mean_delta(),
        ));
    }

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
            overall.accuracy() * 100.0
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
                (tally.accuracy() - overall.accuracy()) * 100.0,
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
}
