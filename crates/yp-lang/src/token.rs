use crate::span::Span;

/// What a chunk of the prompt looks like.
///
/// The distinction that matters most downstream is `Ident` / `Path` / `CodeSpan`
/// versus prose: those are the candidate *referents* that the grounding axis
/// tries to resolve against the repository index, while prose is what the
/// smell matcher reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// An ordinary Latin-script word.
    Word,
    /// A run of Hangul syllables.
    Hangul,
    /// A bare number.
    Number,
    /// Looks like a code identifier: snake_case, camelCase, PascalCase,
    /// or dotted like `mod.attr`.
    Ident,
    /// Looks like a file path: contains a `/` or `\`.
    Path,
    /// Text that was inside backticks or a fenced block.
    CodeSpan,
    /// Anything else.
    Punct,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    /// The token text. Lowercased for `Word`; verbatim for everything else,
    /// because identifier casing is meaningful when resolving against a repo.
    pub text: String,
}

impl Token {
    /// True for tokens that could name something in the repository.
    pub fn is_referent_candidate(&self) -> bool {
        matches!(
            self.kind,
            TokenKind::Ident | TokenKind::Path | TokenKind::CodeSpan
        )
    }
}

/// Words that are programming-language syntax rather than names.
///
/// Inside a pasted snippet every bare word is a name in principle, but
/// `import` and `return` name nothing a user could be pointing at. They exist
/// in every repository equally, so they only dilute the measurement.
const CODE_KEYWORDS: &[&str] = &[
    "if",
    "else",
    "elif",
    "for",
    "while",
    "do",
    "switch",
    "case",
    "default",
    "break",
    "continue",
    "return",
    "yield",
    "await",
    "async",
    "try",
    "catch",
    "except",
    "finally",
    "raise",
    "throw",
    "throws",
    "import",
    "from",
    "export",
    "require",
    "include",
    "use",
    "using",
    "package",
    "namespace",
    "def",
    "class",
    "fn",
    "func",
    "function",
    "let",
    "var",
    "const",
    "static",
    "final",
    "public",
    "private",
    "protected",
    "abstract",
    "virtual",
    "override",
    "extends",
    "implements",
    "interface",
    "struct",
    "enum",
    "trait",
    "impl",
    "type",
    "typedef",
    "mod",
    "pub",
    "crate",
    "super",
    "self",
    "this",
    "new",
    "delete",
    "null",
    "nil",
    "none",
    "true",
    "false",
    "and",
    "or",
    "not",
    "in",
    "is",
    "as",
    "with",
    "pass",
    "lambda",
    "match",
    "where",
    "when",
    "then",
    "end",
    "begin",
    "void",
    "int",
    "str",
    "bool",
    "float",
    "double",
    "char",
    "long",
    "short",
    "unsigned",
    "print",
    "println",
    "echo",
    "assert",
    "del",
    "global",
    "nonlocal",
    "raise",
];

/// True for a word that is language syntax rather than something named.
pub fn is_code_keyword(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    CODE_KEYWORDS.contains(&lower.as_str())
}

pub fn is_hangul(c: char) -> bool {
    matches!(c as u32,
        0xAC00..=0xD7A3   // syllables
        | 0x1100..=0x11FF // jamo
        | 0x3130..=0x318F // compatibility jamo
    )
}

/// Characters that may appear inside an unbroken identifier/path chunk.
fn is_chunk_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '\\' | '$' | '+' | '#' | '~')
}

/// Locate regions that are code rather than prose: fenced blocks and inline
/// backtick spans. Returned sorted and non-overlapping so `span::covered_by`
/// can binary-search them.
///
/// Unterminated fences and backticks run to end of input, which is the
/// forgiving reading: a prompt ending mid-snippet is still mostly code.
pub fn code_regions(text: &str) -> Vec<Span> {
    let bytes = text.as_bytes();
    let mut regions = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        // Count the run of backticks that opens this region.
        let fence_start = i;
        let mut ticks = 0usize;
        while i < bytes.len() && bytes[i] == b'`' {
            ticks += 1;
            i += 1;
        }
        // Find a closing run of at least the same length.
        let mut end = None;
        let mut j = i;
        while j < bytes.len() {
            if bytes[j] == b'`' {
                let mut run = 0usize;
                while j < bytes.len() && bytes[j] == b'`' {
                    run += 1;
                    j += 1;
                }
                if run >= ticks {
                    end = Some(j);
                    break;
                }
            } else {
                j += 1;
            }
        }
        let region_end = end.unwrap_or(bytes.len());
        regions.push(Span::new(fence_start, region_end));
        i = region_end;
    }
    regions
}

fn classify(chunk: &str) -> TokenKind {
    if chunk.contains('/') || chunk.contains('\\') {
        return TokenKind::Path;
    }
    if chunk.chars().all(|c| c.is_ascii_digit()) {
        return TokenKind::Number;
    }
    let has_alpha = chunk.chars().any(|c| c.is_ascii_alphabetic());
    if !has_alpha {
        return TokenKind::Punct;
    }
    if chunk.contains('_') {
        return TokenKind::Ident;
    }
    // camelCase / PascalCase: an uppercase letter that follows a lowercase one.
    let mut prev_lower = false;
    for c in chunk.chars() {
        if c.is_ascii_uppercase() && prev_lower {
            return TokenKind::Ident;
        }
        prev_lower = c.is_ascii_lowercase();
    }
    // Dotted, with word characters on both sides of a dot: `mod.attr`, `a.b.c`.
    if let Some(dot) = chunk.find('.') {
        let (before, after) = chunk.split_at(dot);
        let after = &after[1..];
        if !before.is_empty()
            && !after.is_empty()
            && before.ends_with(|c: char| c.is_ascii_alphanumeric())
            && after.starts_with(|c: char| c.is_ascii_alphanumeric())
        {
            return TokenKind::Ident;
        }
    }
    TokenKind::Word
}

/// Split a prompt into tokens.
///
/// Code regions become single `CodeSpan` tokens rather than being torn apart,
/// so that a snippet the user pasted is treated as one referent and its prose
/// never reaches the smell matcher.
pub fn tokenize(text: &str) -> Vec<Token> {
    let code = code_regions(text);
    let mut tokens = Vec::new();
    let mut it = text.char_indices().peekable();

    while let Some(&(idx, c)) = it.peek() {
        // Emit any code region whole.
        if let Some(region) = code.iter().find(|r| r.start == idx) {
            let inner = region.slice(text).trim_matches('`').trim().to_string();
            tokens.push(Token {
                kind: TokenKind::CodeSpan,
                span: *region,
                text: inner,
            });
            while it.peek().is_some_and(|&(i, _)| i < region.end) {
                it.next();
            }
            continue;
        }

        if c.is_whitespace() {
            it.next();
            continue;
        }

        if is_hangul(c) {
            let start = idx;
            let mut end = idx;
            while let Some(&(i, ch)) = it.peek() {
                if is_hangul(ch) {
                    end = i + ch.len_utf8();
                    it.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                kind: TokenKind::Hangul,
                span: Span::new(start, end),
                text: text[start..end].to_string(),
            });
            continue;
        }

        if is_chunk_char(c) {
            let start = idx;
            let mut end = idx;
            while let Some(&(i, ch)) = it.peek() {
                // Stop before a code region so we never swallow its opening tick.
                if code.iter().any(|r| r.start == i) {
                    break;
                }
                if is_chunk_char(ch) {
                    end = i + ch.len_utf8();
                    it.next();
                } else {
                    break;
                }
            }
            // Trailing sentence punctuation is not part of the token.
            while end > start
                && (text[start..end].ends_with('.') || text[start..end].ends_with('-'))
            {
                end -= 1;
            }
            if end == start {
                continue;
            }
            let chunk = &text[start..end];
            let kind = classify(chunk);
            let text_out = if kind == TokenKind::Word {
                chunk.to_ascii_lowercase()
            } else {
                chunk.to_string()
            };
            tokens.push(Token {
                kind,
                span: Span::new(start, end),
                text: text_out,
            });
            continue;
        }

        // Everything else is punctuation.
        let start = idx;
        it.next();
        tokens.push(Token {
            kind: TokenKind::Punct,
            span: Span::new(start, start + c.len_utf8()),
            text: c.to_string(),
        });
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(TokenKind, String)> {
        tokenize(text)
            .into_iter()
            .map(|t| (t.kind, t.text))
            .collect()
    }

    #[test]
    fn classifies_identifiers_and_paths() {
        let got = kinds("fix verify_token in src/auth/login.rs now");
        assert!(got.contains(&(TokenKind::Ident, "verify_token".into())));
        assert!(got.contains(&(TokenKind::Path, "src/auth/login.rs".into())));
        assert!(got.contains(&(TokenKind::Word, "fix".into())));
    }

    #[test]
    fn camel_case_is_an_identifier_but_a_plain_word_is_not() {
        assert_eq!(classify("parseConfig"), TokenKind::Ident);
        assert_eq!(classify("ParseConfig"), TokenKind::Ident);
        assert_eq!(classify("handler"), TokenKind::Word);
        assert_eq!(classify("HTTP"), TokenKind::Word);
    }

    #[test]
    fn code_spans_stay_whole() {
        let toks = tokenize("call `foo.bar(1, 2)` please");
        let code: Vec<_> = toks
            .iter()
            .filter(|t| t.kind == TokenKind::CodeSpan)
            .collect();
        assert_eq!(code.len(), 1);
        assert_eq!(code[0].text, "foo.bar(1, 2)");
    }

    #[test]
    fn unterminated_backtick_runs_to_end() {
        let regions = code_regions("open `foo");
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].end, "open `foo".len());
    }

    #[test]
    fn hangul_splits_from_latin_at_the_boundary() {
        let got = kinds("verify_token이 panic 남");
        assert!(got.contains(&(TokenKind::Ident, "verify_token".into())));
        assert!(got.contains(&(TokenKind::Hangul, "이".into())));
        assert!(got.contains(&(TokenKind::Word, "panic".into())));
    }

    #[test]
    fn language_syntax_is_recognised_as_such() {
        assert!(is_code_keyword("import"));
        assert!(is_code_keyword("RETURN"));
        assert!(is_code_keyword("def"));
        assert!(!is_code_keyword("ccode"));
        assert!(!is_code_keyword("separability_matrix"));
    }

    #[test]
    fn trailing_sentence_dot_is_trimmed() {
        let got = kinds("edit main.rs.");
        assert!(got.contains(&(TokenKind::Ident, "main.rs".into())));
    }

    #[test]
    fn spans_land_on_char_boundaries() {
        let text = "그거 좀 고쳐줘 `code` 그리고 src/a.rs 도";
        for t in tokenize(text) {
            assert!(text.is_char_boundary(t.span.start));
            assert!(text.is_char_boundary(t.span.end));
        }
    }
}
