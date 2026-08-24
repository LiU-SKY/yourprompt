/// Letter grade for a 0-1000 score.
///
/// The bands are deliberately generous at the top and unforgiving at the
/// bottom, because the interesting signal is "this prompt is going to be
/// misread" rather than "this prompt is a 9/10 instead of an 8/10".
///
/// These cutoffs are provisional until `yp bench` calibrates the score
/// distribution against real prompts; see `params` for the same caveat.
pub fn grade(total: f64) -> &'static str {
    match total {
        t if t >= 900.0 => "S",
        t if t >= 850.0 => "A+",
        t if t >= 800.0 => "A",
        t if t >= 750.0 => "A-",
        t if t >= 700.0 => "B+",
        t if t >= 650.0 => "B",
        t if t >= 600.0 => "B-",
        t if t >= 550.0 => "C+",
        t if t >= 500.0 => "C",
        t if t >= 450.0 => "C-",
        t if t >= 400.0 => "D+",
        t if t >= 350.0 => "D",
        _ => "F",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_the_whole_range_without_gaps() {
        let mut t = 0.0;
        while t <= 1000.0 {
            assert!(!grade(t).is_empty(), "no grade at {t}");
            t += 0.5;
        }
    }

    #[test]
    fn grades_are_monotonic() {
        // Walking upward must never move to a grade seen earlier.
        let mut seen: Vec<&str> = Vec::new();
        let mut t = 0.0;
        while t <= 1000.0 {
            let g = grade(t);
            if seen.last() != Some(&g) {
                assert!(!seen.contains(&g), "grade {g} reappeared at {t}");
                seen.push(g);
            }
            t += 0.5;
        }
        assert_eq!(seen.first(), Some(&"F"));
        assert_eq!(seen.last(), Some(&"S"));
    }

    #[test]
    fn boundaries_land_on_the_higher_grade() {
        assert_eq!(grade(800.0), "A");
        assert_eq!(grade(799.9), "A-");
        assert_eq!(grade(1000.0), "S");
        assert_eq!(grade(0.0), "F");
    }
}
