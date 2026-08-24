//! `yp bench` -- does the score actually mean anything?
//!
//! Every prompt scorer on GitHub asserts that its number reflects prompt
//! quality. None of them checks. This does, against two datasets that between
//! them cover the two halves of the claim.
//!
//! [`humanevalcomm`] answers *does the score fall when a specification is
//! deliberately damaged*, using prompts someone else perturbed in known ways.
//!
//! [`swe`] answers the question that matters most here and that no
//! text-only dataset can: *is the grounding axis actually reading the
//! repository*. It scores real GitHub issues against their own codebase and
//! against unrelated ones. If the number does not fall when the repository is
//! swapped, axis A is measuring nothing repository-specific, and the central
//! claim of this project is false.
//!
//! Datasets are downloaded on first use and cached rather than vendored, so
//! the repository stays light and tracks upstream.

pub mod humanevalcomm;
pub mod swe;

use std::path::PathBuf;
use std::process::Command;

use yp_core::Score;

use crate::session;

/// Where downloaded datasets and derived indexes live.
pub fn cache_dir() -> Option<PathBuf> {
    session::state_dir().map(|d| d.join("bench"))
}

/// Fetch `url` to `path` unless it is already there.
///
/// Shelling out to curl rather than linking an HTTP stack: benchmarking is a
/// development command, and the alternative is putting rustls into a binary
/// that runs on every keystroke.
pub fn download(url: &str, path: &PathBuf, refresh: bool) -> Result<(), String> {
    if path.exists() && !refresh {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("part");
    let status = Command::new("curl")
        .args(["-fsSL", url, "-o"])
        .arg(&tmp)
        .status()
        .map_err(|e| format!("could not run curl: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("curl failed downloading {url}"));
    }
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Totals for one axis-ablation setting.
#[derive(Default)]
pub struct Tally {
    pub correct: usize,
    pub ties: usize,
    pub total: usize,
    pub delta_sum: f64,
}

impl Tally {
    pub fn add(&mut self, original: f64, perturbed: f64) {
        self.total += 1;
        self.delta_sum += original - perturbed;
        if (original - perturbed).abs() < 1e-9 {
            self.ties += 1;
        } else if original > perturbed {
            self.correct += 1;
        }
    }

    pub fn accuracy(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.correct as f64 / self.total as f64
    }

    pub fn mean_delta(&self) -> f64 {
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
pub fn total_without(score: &Score, drop: &str) -> f64 {
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
pub fn spearman(pairs: &[(f64, f64)]) -> f64 {
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
    }
}
