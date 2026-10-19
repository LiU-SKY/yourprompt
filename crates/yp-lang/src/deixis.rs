//! Which vague pronouns stand for a thing the reader could be shown.
//!
//! The `vague_pronoun` smell charges every "it", "this" and "그거", because on
//! its own a pronoun names nothing. But "fix this" above a pasted stack trace
//! names the trace, and that is how people ask for fixes. A pronoun that
//! stands for a thing -- a demonstrative, "it", "they" -- has something to
//! stand for once the prompt carries a block of code or output.
//!
//! Left out: pronouns that reach back into the conversation rather than at
//! the page ("the same", "the former", 아까, 방금, 그때), since a paste does not
//! say what was discussed an hour ago; and places ("there", 여기, 거기),
//! which in a question are usually existential ("is there a test for it?")
//! rather than the thing asked about.

/// English pronouns that can stand for pasted material. Sorted, for binary
/// search.
///
/// "that" is one only where it points, as "this" does: yp-core takes the
/// relative "a function that returns" and the "that" of "make sure that" out of
/// the vague pronouns before this is asked.
const EN_DEICTICS: &[&str] = &[
    "it",
    "its",
    "that",
    "the above",
    "their",
    "them",
    "these",
    "they",
    "this",
    "those",
];

/// Korean pronouns that can stand for pasted material, including the
/// contracted forms (이건, 이게, 이걸) the lexicon may not list yet. Sorted,
/// for binary search.
const KO_DEICTICS: &[&str] = &[
    "걔",
    "그 부분",
    "그거",
    "그건",
    "그걸",
    "그것",
    "그게",
    "얘",
    "위에 거",
    "이거",
    "이건",
    "이걸",
    "이것",
    "이게",
    "쟤",
    "저 부분",
    "저거",
    "저건",
    "저걸",
    "저것",
    "저게",
];

/// True when `term`, a vague-pronoun hit, can stand for pasted material.
pub fn is_deictic(term: &str) -> bool {
    let lower = term.to_lowercase();
    EN_DEICTICS.binary_search(&lower.as_str()).is_ok()
        || KO_DEICTICS.binary_search(&lower.as_str()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_word_lists_are_sorted_so_the_search_works() {
        for list in [EN_DEICTICS, KO_DEICTICS] {
            let mut sorted = list.to_vec();
            sorted.sort_unstable();
            assert_eq!(list, sorted.as_slice());
        }
    }

    #[test]
    fn demonstratives_point_and_conversation_history_does_not() {
        for term in [
            "this", "This", "it", "that", "these", "those", "이거", "이것", "저거", "그거",
        ] {
            assert!(is_deictic(term), "{term} should point");
        }
        for term in ["the same", "the former", "아까", "방금", "그때"] {
            assert!(!is_deictic(term), "{term} reaches back, not at the page");
        }
        for term in ["there", "여기", "거기"] {
            assert!(!is_deictic(term), "{term} is a place, not a thing");
        }
    }

    #[test]
    fn every_listed_deictic_is_a_vague_pronoun_or_a_form_of_one() {
        // The list narrows the smell; it must not invent pronouns the smell
        // never charges. Contracted Korean forms are the exception, kept so
        // that adding them to the lexicon needs no change here.
        let lexicon = crate::lexicon::load_bundled().unwrap();
        let pronouns: Vec<&str> = lexicon
            .table
            .terms
            .iter()
            .zip(&lexicon.table.categories)
            .filter(|(_, id)| **id == crate::SmellId::VaguePronoun)
            .map(|(t, _)| t.as_str())
            .collect();
        let contracted = [
            "이건", "이게", "이걸", "그건", "그게", "그걸", "저건", "저게", "저걸",
        ];
        for term in EN_DEICTICS.iter().chain(KO_DEICTICS) {
            assert!(
                pronouns.contains(term) || contracted.contains(term),
                "{term} is not a vague pronoun"
            );
        }
    }
}
