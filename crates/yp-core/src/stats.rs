use std::collections::HashSet;

use yp_lang::{Token, TokenKind};

/// Structural facts about a prompt, computed once and shared by every axis.
#[derive(Debug, Clone)]
pub struct PromptStats {
    /// Tokens that carry meaning: everything except punctuation.
    pub content_tokens: usize,
    /// How many of those are distinct, for the lexical-variety measure.
    pub distinct_content: usize,
    /// Whether the prompt pastes any code, inline or fenced.
    pub has_code: bool,
    /// Lines that begin a bullet or numbered list item.
    pub list_lines: usize,
    /// Tokens that could name something in the repository. The grounding axis
    /// resolves these; until it exists they are collected but unused.
    pub referents: Vec<String>,
}

impl PromptStats {
    /// Root type-token ratio: distinct tokens over the square root of total.
    ///
    /// Plain TTR falls as text gets longer no matter how varied it is, which
    /// would penalise detailed prompts for being detailed. Dividing by the
    /// square root is the standard correction and keeps the measure roughly
    /// flat across lengths.
    pub fn root_ttr(&self) -> f64 {
        if self.content_tokens == 0 {
            return 0.0;
        }
        self.distinct_content as f64 / (self.content_tokens as f64).sqrt()
    }
}

/// True for a line that opens a bullet or numbered list item.
fn is_list_line(line: &str) -> bool {
    let t = line.trim_start();
    if let Some(rest) = t
        .strip_prefix("- ")
        .or_else(|| t.strip_prefix("* "))
        .or_else(|| t.strip_prefix("+ "))
    {
        return !rest.trim().is_empty();
    }
    // "1. ", "2) ", "10. "
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || digits.len() > 3 {
        return false;
    }
    let rest = &t[digits.len()..];
    matches!(rest.chars().next(), Some('.') | Some(')'))
        && rest.len() > 1
        && rest[1..].starts_with(' ')
        && !rest[1..].trim().is_empty()
}

pub fn analyze(text: &str, tokens: &[Token]) -> PromptStats {
    let mut distinct: HashSet<&str> = HashSet::new();
    let mut content_tokens = 0usize;
    let mut has_code = false;
    let mut referents = Vec::new();

    for token in tokens {
        match token.kind {
            TokenKind::Punct => continue,
            TokenKind::CodeSpan => has_code = true,
            _ => {}
        }
        content_tokens += 1;
        distinct.insert(token.text.as_str());
        if token.is_referent_candidate() {
            referents.push(token.text.clone());
        }
    }

    let list_lines = text.lines().filter(|l| is_list_line(l)).count();

    PromptStats {
        content_tokens,
        distinct_content: distinct.len(),
        has_code,
        list_lines,
        referents,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yp_lang::tokenize;

    fn stats(text: &str) -> PromptStats {
        analyze(text, &tokenize(text))
    }

    #[test]
    fn punctuation_is_not_content() {
        let s = stats("fix it, now!");
        assert_eq!(s.content_tokens, 3);
    }

    #[test]
    fn detects_pasted_code() {
        assert!(stats("run `cargo test` first").has_code);
        assert!(!stats("run cargo test first").has_code);
    }

    #[test]
    fn counts_bullet_and_numbered_lists() {
        let text = "do these:\n- first thing\n- second thing\n1. third\n2) fourth\nnot a list";
        assert_eq!(stats(text).list_lines, 4);
    }

    #[test]
    fn a_bare_dash_or_number_is_not_a_list() {
        assert_eq!(stats("- \n1.\n42 items\n-5 degrees").list_lines, 0);
    }

    #[test]
    fn root_ttr_penalises_repetition_but_not_length() {
        let varied = stats("refactor the parser so it returns a typed config value");
        let repetitive = stats("fix fix fix fix fix fix fix fix fix fix");
        assert!(
            varied.root_ttr() > repetitive.root_ttr(),
            "varied {} vs repetitive {}",
            varied.root_ttr(),
            repetitive.root_ttr()
        );
    }

    #[test]
    fn root_ttr_of_empty_text_is_zero() {
        assert_eq!(stats("").root_ttr(), 0.0);
    }

    #[test]
    fn collects_identifier_and_path_referents() {
        let s = stats("fix verify_token in src/auth/login.rs");
        assert!(s.referents.iter().any(|r| r == "verify_token"));
        assert!(s.referents.iter().any(|r| r == "src/auth/login.rs"));
        assert!(!s.referents.iter().any(|r| r == "fix"));
    }
}
