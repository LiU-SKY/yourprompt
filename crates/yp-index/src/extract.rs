//! Pulling identifiers out of source text.
//!
//! No parser. The grounding axis asks two questions -- "how many places in
//! this repository could this name refer to?" and "how specific is this word
//! here?" -- and both are answered by corpus statistics, which is exactly what
//! the pre-retrieval query-performance-prediction literature the axis is
//! modelled on uses (Hauff et al., CIKM 2008). A parser would add precision to
//! only one part of one sub-score, at the cost of a grammar per language.
//!
//! What we do borrow from parsing is a cheap definition heuristic: a keyword
//! such as `fn`, `class` or `def` followed by an identifier is very likely a
//! definition site. That is enough to tell "defined once, used everywhere"
//! apart from "genuinely ambiguous".

use std::collections::HashMap;

/// Keywords that introduce a named definition, across the languages a coding
/// agent is likely to meet. Order does not matter; each is matched as a whole
/// token, and the next identifier on the line is taken as the name.
const DEFINITION_KEYWORDS: &[&str] = &[
    // Rust
    "fn",
    "struct",
    "enum",
    "trait",
    "impl",
    "mod",
    "macro_rules",
    "union",
    // C, C++, C#, Java, Kotlin, Swift, Scala
    "class",
    "interface",
    "namespace",
    "record",
    "protocol",
    "extension",
    "typedef",
    "object",
    // Python, Ruby
    "def",
    "module",
    // JS, TS
    "function",
    "type", // Go
    "func",
    "package", // Shell, Perl, PHP, SQL
    "sub",
    "procedure",
    "proc",
    "table",
    "view", // Generic
    "const",
    "constant",
    "define",
    "component",
    "service",
    "resource",
];

/// Statistics for one term across the repository.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TermStat {
    /// Document frequency: how many files contain it. This is the number the
    /// `resolve@1` sub-score reads -- one file is a clean referent, forty is
    /// a prompt the agent cannot pin down.
    pub df: u32,
    /// Collection frequency: total occurrences, for the specificity model.
    pub cf: u32,
    /// How many times it appeared in a definition position.
    pub def: u32,
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Split an identifier into its lowercase word parts.
///
/// `verify_token` and `verifyToken` and `VerifyToken` all yield
/// `["verify", "token"]`. This is what lets a prompt saying "the login
/// handler" resolve against `handle_login`, and equally what makes "login"
/// register as ambiguous when it appears in forty files.
pub fn subwords(ident: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = ident.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        if c == '_' || c == '-' {
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current));
            }
            continue;
        }
        if c.is_ascii_uppercase() && !current.is_empty() {
            let prev = chars[i - 1];
            let next_is_lower = chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase());
            // Break before an uppercase that starts a new word: "fooBar", and
            // the tail of an acronym run such as "HTTPServer" -> HTTP, Server.
            if prev.is_ascii_lowercase() || prev.is_ascii_digit() || next_is_lower {
                parts.push(std::mem::take(&mut current));
            }
        }
        current.push(c.to_ascii_lowercase());
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts.retain(|p| p.len() > 1);
    parts
}

/// Extract every identifier from one file, tallying document and collection
/// frequency and noting definition positions.
///
/// `into` is updated in place so one map accumulates the whole repository.
pub fn index_text(text: &str, into: &mut HashMap<String, TermStat>) {
    // Terms seen in this file, so document frequency counts files not hits.
    let mut seen_here: HashMap<String, bool> = HashMap::new();

    for line in text.lines() {
        let mut expecting_definition = false;
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0usize;

        while i < chars.len() {
            if !is_ident_start(chars[i]) {
                i += 1;
                continue;
            }
            let start = i;
            while i < chars.len() && is_ident_char(chars[i]) {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();

            if DEFINITION_KEYWORDS.contains(&ident.as_str()) {
                expecting_definition = true;
                continue;
            }

            let is_definition = expecting_definition;
            expecting_definition = false;

            let lower = ident.to_lowercase();
            record(into, &mut seen_here, &lower, is_definition);
            for part in subwords(&ident) {
                // A single-word identifier is its own only subword. Recording
                // it again would count every occurrence twice and inflate the
                // frequencies the whole specificity model rests on.
                if part == lower {
                    continue;
                }
                // A subword is never itself a definition -- only the whole
                // identifier is. Counting it as one would make every word of
                // every function name look like a declaration.
                record(into, &mut seen_here, &part, false);
            }
        }
    }
}

fn record(
    into: &mut HashMap<String, TermStat>,
    seen_here: &mut HashMap<String, bool>,
    term: &str,
    is_definition: bool,
) {
    if term.len() < 2 || term.len() > 64 {
        return;
    }
    let stat = into.entry(term.to_string()).or_default();
    stat.cf = stat.cf.saturating_add(1);
    if is_definition {
        stat.def = stat.def.saturating_add(1);
    }
    if !seen_here.contains_key(term) {
        seen_here.insert(term.to_string(), true);
        stat.df = stat.df.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(text: &str) -> HashMap<String, TermStat> {
        let mut map = HashMap::new();
        index_text(text, &mut map);
        map
    }

    #[test]
    fn splits_snake_camel_and_pascal_case() {
        assert_eq!(subwords("verify_token"), ["verify", "token"]);
        assert_eq!(subwords("verifyToken"), ["verify", "token"]);
        assert_eq!(subwords("VerifyToken"), ["verify", "token"]);
        assert_eq!(subwords("handle-login"), ["handle", "login"]);
    }

    #[test]
    fn splits_acronym_runs_at_the_right_place() {
        assert_eq!(subwords("HTTPServer"), ["http", "server"]);
        assert_eq!(subwords("parseHTTPResponse"), ["parse", "http", "response"]);
    }

    #[test]
    fn drops_single_letter_fragments() {
        // "aB" would otherwise contribute a meaningless "a".
        assert_eq!(subwords("aButton"), ["button"]);
        assert!(subwords("x").is_empty());
    }

    #[test]
    fn counts_documents_not_occurrences_for_df() {
        let mut map = HashMap::new();
        index_text("token token token", &mut map);
        let stat = map["token"];
        assert_eq!(stat.df, 1, "one file");
        assert_eq!(stat.cf, 3, "three occurrences");

        index_text("token", &mut map);
        assert_eq!(map["token"].df, 2, "now two files");
        assert_eq!(map["token"].cf, 4);
    }

    #[test]
    fn recognises_definition_sites_across_languages() {
        for (source, name) in [
            ("pub fn verify_token(t: &str) {}", "verify_token"),
            ("class LoginHandler:", "loginhandler"),
            ("def handle_login(self):", "handle_login"),
            ("export function parseConfig() {}", "parseconfig"),
            ("func ServeHTTP(w http.ResponseWriter) {}", "servehttp"),
            ("type Claims struct {}", "claims"),
        ] {
            let map = index(source);
            assert!(
                map.get(name).is_some_and(|s| s.def >= 1),
                "{source:?} did not register {name} as a definition: {:?}",
                map.get(name)
            );
        }
    }

    #[test]
    fn a_use_is_not_a_definition() {
        let map = index("verify_token(input);");
        assert_eq!(map["verify_token"].def, 0);
        assert_eq!(map["verify_token"].cf, 1);
    }

    #[test]
    fn a_single_word_identifier_is_counted_once_not_twice() {
        // Regression: "token" is its own only subword, and was being recorded
        // both as the identifier and as the subword, doubling every frequency
        // the specificity model reads.
        let map = index("token");
        assert_eq!(map["token"].cf, 1);
        assert_eq!(map["token"].df, 1);
    }

    #[test]
    fn subwords_are_indexed_alongside_the_whole_identifier() {
        let map = index("fn verify_token() {}");
        assert!(map.contains_key("verify_token"));
        assert!(map.contains_key("verify"));
        assert!(map.contains_key("token"));
        // Only the full identifier is the definition.
        assert_eq!(map["verify_token"].def, 1);
        assert_eq!(map["verify"].def, 0);
    }

    #[test]
    fn keywords_themselves_are_not_indexed_as_terms() {
        let map = index("fn main() {}");
        assert!(!map.contains_key("fn"), "keyword leaked into the index");
        assert!(map.contains_key("main"));
    }

    #[test]
    fn handles_empty_and_non_source_text_without_panicking() {
        assert!(index("").is_empty());
        assert!(index("   \n\n\t").is_empty());
        index("🎉 한국어 텍스트 ///");
    }
}
