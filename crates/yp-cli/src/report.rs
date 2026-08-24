use std::io::IsTerminal;

use yp_core::{Axis, Score};

/// Width of the progress bars, in cells.
const BAR_CELLS: usize = 10;

const FILLED: char = '▓';
const EMPTY: char = '░';

/// ANSI styling, switched off when output is not a terminal or the user has
/// asked for no colour.
///
/// Honours `NO_COLOR` (any value) and `CLICOLOR_FORCE`, the two conventions
/// most CLI tools agree on.
#[derive(Clone, Copy)]
pub struct Style {
    enabled: bool,
}

impl Style {
    pub fn detect() -> Self {
        let forced = std::env::var_os("CLICOLOR_FORCE").is_some_and(|v| v != "0");
        let disabled = std::env::var_os("NO_COLOR").is_some();
        Style {
            enabled: forced || (!disabled && std::io::stdout().is_terminal()),
        }
    }

    pub fn plain() -> Self {
        Style { enabled: false }
    }

    /// Colour on regardless of whether stdout is a terminal.
    ///
    /// The status line command's stdout is captured by Claude Code rather
    /// than attached to a tty, so detection would always answer "no colour"
    /// even though Claude Code renders ANSI perfectly well. `NO_COLOR` is
    /// still honoured.
    pub fn forced() -> Self {
        Style {
            enabled: std::env::var_os("NO_COLOR").is_none(),
        }
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn dim(&self, text: &str) -> String {
        self.paint("2", text)
    }

    pub fn bold(&self, text: &str) -> String {
        self.paint("1", text)
    }

    /// Green / yellow / red by how much of the maximum was earned.
    pub fn by_ratio(&self, ratio: f64, text: &str) -> String {
        let code = if ratio >= 0.70 {
            "32"
        } else if ratio >= 0.50 {
            "33"
        } else {
            "31"
        };
        self.paint(code, text)
    }
}

pub fn bar(ratio: f64) -> String {
    let filled = (ratio.clamp(0.0, 1.0) * BAR_CELLS as f64).round() as usize;
    let mut s = String::with_capacity(BAR_CELLS * 3);
    for i in 0..BAR_CELLS {
        s.push(if i < filled { FILLED } else { EMPTY });
    }
    s
}

/// The one-line form, as it appears in the status line.
pub fn one_line(score: &Score, style: &Style) -> String {
    let ratio = score.total / 1000.0;
    let head = format!(
        "{:.1} {} {}",
        score.display_total(),
        bar(ratio),
        score.grade
    );
    let mut out = style.by_ratio(ratio, &head);
    if score.renormalized {
        // Never let a renormalised score pass for a grounded one.
        out.push_str(&style.dim(" ~"));
    }
    out
}

fn axis_block(axis: &Axis, style: &Style) -> String {
    let ratio = axis.earned / axis.max;
    let mut out = format!(
        "  {:<16}{:>7.1} / {:<6.0} {}\n",
        style.bold(axis.id),
        axis.earned,
        axis.max,
        style.by_ratio(ratio, &bar(ratio))
    );
    for component in &axis.components {
        out.push_str(&format!(
            "    {:<14}{:>7.1} / {:<6.0} {}\n",
            component.id,
            component.earned,
            component.max,
            style.dim(&component.detail)
        ));
    }
    out
}

/// The full audit, as `yp score` prints it.
///
/// Every sub-score is shown with the reason it came out that way. The model is
/// meant to be argued with, which it cannot be if it only prints a number.
pub fn full(score: &Score, style: &Style) -> String {
    let ratio = score.total / 1000.0;
    let mut out = String::new();

    out.push('\n');
    out.push_str(&format!(
        "  {}  {}  {}\n\n",
        style.bold(&format!("{:.1} / 1000", score.display_total())),
        style.by_ratio(ratio, score.grade),
        style.by_ratio(ratio, &bar(ratio)),
    ));

    match &score.grounding {
        Some(axis) => out.push_str(&axis_block(axis, style)),
        None => out.push_str(&format!(
            "  {:<16}{}\n",
            style.bold("grounding"),
            style.dim("not scored -- no repository index (axis A lands in M3)")
        )),
    }
    out.push_str(&axis_block(&score.actionability, style));
    out.push_str(&axis_block(&score.clarity, style));
    out.push_str(&axis_block(&score.context, style));

    if score.renormalized {
        out.push_str(&format!(
            "\n  {}\n",
            style.dim(
                "~ grounding unavailable; the remaining axes were rescaled to 1000, \
                 so this is not comparable to a grounded score."
            )
        ));
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(text: &str) -> Score {
        yp_core::score(text).unwrap()
    }

    #[test]
    fn bar_is_always_the_declared_width() {
        for ratio in [-1.0, 0.0, 0.37, 0.5, 1.0, 2.0] {
            assert_eq!(bar(ratio).chars().count(), BAR_CELLS, "ratio {ratio}");
        }
    }

    #[test]
    fn bar_fills_from_empty_to_full() {
        assert!(bar(0.0).chars().all(|c| c == EMPTY));
        assert!(bar(1.0).chars().all(|c| c == FILLED));
    }

    #[test]
    fn plain_style_emits_no_escape_sequences() {
        let s = full(&score("refactor parse_args in src/cli.rs"), &Style::plain());
        assert!(!s.contains('\x1b'), "found ANSI escape in plain output");
    }

    #[test]
    fn one_line_marks_a_renormalised_score() {
        let line = one_line(&score("refactor parse_args"), &Style::plain());
        assert!(line.ends_with(" ~"), "got {line:?}");
    }

    #[test]
    fn full_report_shows_every_axis_and_component() {
        let s = score("refactor parse_args in src/cli.rs so it returns Config; tests must pass");
        let text = full(&s, &Style::plain());
        for axis in ["grounding", "actionability", "clarity", "context"] {
            assert!(text.contains(axis), "missing axis {axis}");
        }
        for component in ["objective", "singularity", "io_spec", "acceptance", "shape"] {
            assert!(text.contains(component), "missing component {component}");
        }
    }
}
