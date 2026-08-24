use serde::Deserialize;
use std::fmt;
use std::str::FromStr;

use crate::matcher::TermTable;

/// Terms whose *presence* is evidence a prompt is executable.
///
/// Where [`crate::lexicon::SmellId`] enumerates what makes a prompt harder to
/// act on, these enumerate what makes it easier. The B (actionability) and D
/// (context sufficiency) axes are built from them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CueId {
    /// A concrete imperative: "refactor", "추가해". Deliberately excludes
    /// polysemous verbs like "handle", which name an action without naming
    /// what the action is.
    ActionVerb,
    /// Names the shape of the result: return values, parameters, status
    /// codes, schemas.
    IoSpec,
    /// States how the work will be judged done: tests, assertions, expected
    /// values, reproduction steps.
    Acceptance,
    /// Bounds the work: "only in", "don't touch", "빼고", "그대로".
    ScopeConstraint,
    /// Introduces a worked example.
    ExampleMarker,
    /// Joins one objective to another. Used by the singularity check, which
    /// is the one cue category that can *cost* points.
    Conjunction,
}

impl CueId {
    pub const ALL: [CueId; 6] = [
        CueId::ActionVerb,
        CueId::IoSpec,
        CueId::Acceptance,
        CueId::ScopeConstraint,
        CueId::ExampleMarker,
        CueId::Conjunction,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            CueId::ActionVerb => "action_verb",
            CueId::IoSpec => "io_spec",
            CueId::Acceptance => "acceptance",
            CueId::ScopeConstraint => "scope_constraint",
            CueId::ExampleMarker => "example_marker",
            CueId::Conjunction => "conjunction",
        }
    }
}

impl fmt::Display for CueId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug)]
pub struct UnknownCue(pub String);

impl fmt::Display for UnknownCue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown cue id: {}", self.0)
    }
}

impl std::error::Error for UnknownCue {}

impl FromStr for CueId {
    type Err = UnknownCue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        CueId::ALL
            .iter()
            .copied()
            .find(|id| id.as_str() == s)
            .ok_or_else(|| UnknownCue(s.to_string()))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CueDef {
    pub id: String,
    pub terms: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CueFile {
    pub lang: String,
    #[serde(rename = "cue", default)]
    pub cues: Vec<CueDef>,
}

#[derive(Debug, Clone)]
pub struct Cues {
    pub table: TermTable<CueId>,
}

impl Cues {
    pub fn from_files(files: &[CueFile]) -> Result<Self, UnknownCue> {
        let mut table = TermTable::default();
        for file in files {
            for def in &file.cues {
                let id: CueId = def.id.parse()?;
                for term in &def.terms {
                    let term = term.trim();
                    if term.is_empty() {
                        continue;
                    }
                    table.push(term, id);
                }
            }
        }
        Ok(Cues { table })
    }
}

pub const EN_TOML: &str = include_str!("../../../data/cues.en.toml");
pub const KO_TOML: &str = include_str!("../../../data/cues.ko.toml");

pub fn load_bundled() -> Result<Cues, Box<dyn std::error::Error>> {
    let en: CueFile = toml::from_str(EN_TOML)?;
    let ko: CueFile = toml::from_str(KO_TOML)?;
    Ok(Cues::from_files(&[en, ko])?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::Matcher;

    #[test]
    fn bundled_cues_parse_and_cover_every_category() {
        let cues = load_bundled().expect("bundled cues must parse");
        for id in CueId::ALL {
            assert!(
                cues.table.categories.contains(&id),
                "category {id} missing from bundled cues"
            );
        }
    }

    #[test]
    fn both_languages_define_all_six_categories() {
        let en: CueFile = toml::from_str(EN_TOML).unwrap();
        let ko: CueFile = toml::from_str(KO_TOML).unwrap();
        assert_eq!(en.lang, "en");
        assert_eq!(ko.lang, "ko");
        assert_eq!(en.cues.len(), 6);
        assert_eq!(ko.cues.len(), 6);
    }

    fn found(text: &str) -> Vec<CueId> {
        let cues = load_bundled().unwrap();
        let m = Matcher::new(&cues.table).unwrap();
        let mut v: Vec<_> = m.find(text).into_iter().map(|h| h.id).collect();
        v.sort();
        v.dedup();
        v
    }

    #[test]
    fn finds_english_cues() {
        let got = found("refactor parse_args so that it returns a Config, only in src/cli.rs, and the tests pass");
        assert!(got.contains(&CueId::ActionVerb), "got {got:?}");
        assert!(got.contains(&CueId::IoSpec), "got {got:?}");
        assert!(got.contains(&CueId::Acceptance), "got {got:?}");
        assert!(got.contains(&CueId::ScopeConstraint), "got {got:?}");
    }

    #[test]
    fn finds_korean_cues() {
        let got = found(
            "src/cli.rs 만 수정해서 parse_args 가 Config 를 반환하도록 바꾸고 테스트 통과시켜줘",
        );
        assert!(got.contains(&CueId::ActionVerb), "got {got:?}");
        assert!(got.contains(&CueId::IoSpec), "got {got:?}");
        assert!(got.contains(&CueId::Acceptance), "got {got:?}");
    }

    #[test]
    fn bare_korean_particle_man_is_not_a_scope_cue() {
        // "만" appears inside "만들어" and "하지만"; only the longer forms
        // "에서만" / "안에서만" are listed, so neither may fire here.
        let got = found("버튼을 만들어 주세요");
        assert!(!got.contains(&CueId::ScopeConstraint), "got {got:?}");
    }

    #[test]
    fn korean_disjunction_ttoneun_is_not_a_conjunction_cue() {
        // "또는" means "or"; bare "또" is deliberately absent from the list.
        let got = found("A 또는 B 중에 골라");
        assert!(!got.contains(&CueId::Conjunction), "got {got:?}");
    }

    #[test]
    fn cue_id_roundtrips_through_its_string_form() {
        for id in CueId::ALL {
            assert_eq!(id.as_str().parse::<CueId>().unwrap(), id);
        }
        assert!("nope".parse::<CueId>().is_err());
    }
}
