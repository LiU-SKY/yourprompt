use serde::Deserialize;

use crate::matcher::TermTable;
use std::fmt;
use std::str::FromStr;

/// The eleven Requirements Smells this crate detects.
///
/// The taxonomy comes from Femmer et al., *Rapid quality assurance with
/// Requirements Smells* (JSS 2017), which derives it from the natural-language
/// quality criteria of ISO/IEC/IEEE 29148. The names are kept close to the
/// paper so the mapping stays auditable; the *terms* behind each name are ours,
/// chosen for prompts to coding agents rather than requirements documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SmellId {
    SubjectiveLanguage,
    Comparative,
    Superlative,
    PassiveVoice,
    UncertainVerb,
    AmbiguousAdverb,
    VagueNoun,
    VaguePronoun,
    OpenEnded,
    Loophole,
    PolysemousVerb,
}

impl SmellId {
    pub const ALL: [SmellId; 11] = [
        SmellId::SubjectiveLanguage,
        SmellId::Comparative,
        SmellId::Superlative,
        SmellId::PassiveVoice,
        SmellId::UncertainVerb,
        SmellId::AmbiguousAdverb,
        SmellId::VagueNoun,
        SmellId::VaguePronoun,
        SmellId::OpenEnded,
        SmellId::Loophole,
        SmellId::PolysemousVerb,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SmellId::SubjectiveLanguage => "subjective_language",
            SmellId::Comparative => "comparative",
            SmellId::Superlative => "superlative",
            SmellId::PassiveVoice => "passive_voice",
            SmellId::UncertainVerb => "uncertain_verb",
            SmellId::AmbiguousAdverb => "ambiguous_adverb",
            SmellId::VagueNoun => "vague_noun",
            SmellId::VaguePronoun => "vague_pronoun",
            SmellId::OpenEnded => "open_ended",
            SmellId::Loophole => "loophole",
            SmellId::PolysemousVerb => "polysemous_verb",
        }
    }

    /// A short human-readable label for the `/score` report.
    pub fn label(self) -> &'static str {
        match self {
            SmellId::SubjectiveLanguage => "subjective language",
            SmellId::Comparative => "unanchored comparative",
            SmellId::Superlative => "unanchored superlative",
            SmellId::PassiveVoice => "passive voice",
            SmellId::UncertainVerb => "uncertain verb",
            SmellId::AmbiguousAdverb => "ambiguous adverb",
            SmellId::VagueNoun => "vague noun",
            SmellId::VaguePronoun => "vague pronoun",
            SmellId::OpenEnded => "open-ended term",
            SmellId::Loophole => "loophole",
            SmellId::PolysemousVerb => "polysemous verb",
        }
    }
}

impl fmt::Display for SmellId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug)]
pub struct UnknownSmell(pub String);

impl fmt::Display for UnknownSmell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown smell id: {}", self.0)
    }
}

impl std::error::Error for UnknownSmell {}

impl FromStr for SmellId {
    type Err = UnknownSmell;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SmellId::ALL
            .iter()
            .copied()
            .find(|id| id.as_str() == s)
            .ok_or_else(|| UnknownSmell(s.to_string()))
    }
}

/// One smell category as it appears in a lexicon TOML file.
#[derive(Debug, Clone, Deserialize)]
pub struct SmellDef {
    pub id: String,
    /// The ISO/IEC/IEEE 29148 criterion this smell violates. Documentation
    /// only -- scoring uses `weight` and `cap`.
    #[serde(default)]
    pub iso: String,
    pub weight: f64,
    pub cap: usize,
    pub terms: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LexiconFile {
    pub lang: String,
    #[serde(rename = "smell", default)]
    pub smells: Vec<SmellDef>,
}

/// A smell category after validation: string id resolved to `SmellId`.
#[derive(Debug, Clone)]
pub struct Category {
    pub id: SmellId,
    pub weight: f64,
    pub cap: usize,
}

/// Every term from every loaded lexicon, flattened, plus the per-category
/// metadata the scorer needs.
#[derive(Debug, Clone)]
pub struct Lexicon {
    /// The literal patterns handed to the matcher, each tagged with its
    /// category.
    pub table: TermTable<SmellId>,
    /// Per-category weight and cap.
    pub categories: Vec<Category>,
}

impl Lexicon {
    pub fn category(&self, id: SmellId) -> Option<&Category> {
        self.categories.iter().find(|c| c.id == id)
    }

    /// Merge parsed lexicon files into one flat term table.
    ///
    /// When two files define the same category (they do -- one per language)
    /// the first file's `weight` and `cap` win, so the English file is the
    /// reference for scoring constants and translations cannot silently
    /// reweight a category.
    pub fn from_files(files: &[LexiconFile]) -> Result<Self, UnknownSmell> {
        let mut table = TermTable::default();
        let mut categories: Vec<Category> = Vec::new();

        for file in files {
            for def in &file.smells {
                let id: SmellId = def.id.parse()?;
                if !categories.iter().any(|c| c.id == id) {
                    categories.push(Category {
                        id,
                        weight: def.weight,
                        cap: def.cap,
                    });
                }
                for term in &def.terms {
                    let term = term.trim();
                    if term.is_empty() {
                        continue;
                    }
                    table.push(term, id);
                }
            }
        }

        Ok(Lexicon { table, categories })
    }
}

pub const EN_TOML: &str = include_str!("../../../data/lexicon.en.toml");
pub const KO_TOML: &str = include_str!("../../../data/lexicon.ko.toml");

/// Parse the two bundled lexicons.
///
/// Returns an error rather than panicking so the binary can degrade to a
/// grounding-only score instead of taking down the hook. A test asserts the
/// bundled data actually parses, so in practice this never fails in a release.
pub fn load_bundled() -> Result<Lexicon, Box<dyn std::error::Error>> {
    let en: LexiconFile = toml::from_str(EN_TOML)?;
    let ko: LexiconFile = toml::from_str(KO_TOML)?;
    Ok(Lexicon::from_files(&[en, ko])?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_lexicons_parse_and_cover_every_category() {
        let lex = load_bundled().expect("bundled lexicons must parse");
        for id in SmellId::ALL {
            assert!(
                lex.category(id).is_some(),
                "category {id} missing from bundled lexicons"
            );
        }
        assert!(lex.table.len() > 200, "got {} terms", lex.table.len());
        assert_eq!(lex.table.terms.len(), lex.table.categories.len());
    }

    #[test]
    fn both_languages_contribute_terms() {
        let en: LexiconFile = toml::from_str(EN_TOML).unwrap();
        let ko: LexiconFile = toml::from_str(KO_TOML).unwrap();
        assert_eq!(en.lang, "en");
        assert_eq!(ko.lang, "ko");
        assert_eq!(en.smells.len(), 11);
        assert_eq!(ko.smells.len(), 11);
    }

    #[test]
    fn no_duplicate_terms_within_a_category() {
        let lex = load_bundled().unwrap();
        for id in SmellId::ALL {
            let mut seen = std::collections::HashSet::new();
            for (term, cat) in lex.table.terms.iter().zip(&lex.table.categories) {
                if *cat == id {
                    assert!(
                        seen.insert(term.to_ascii_lowercase()),
                        "duplicate term {term:?} in {id}"
                    );
                }
            }
        }
    }

    #[test]
    fn smell_id_roundtrips_through_its_string_form() {
        for id in SmellId::ALL {
            assert_eq!(id.as_str().parse::<SmellId>().unwrap(), id);
        }
        assert!("nonsense".parse::<SmellId>().is_err());
    }
}
