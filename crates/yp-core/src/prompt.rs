//! Telling an instruction apart from the material attached to it.
//!
//! People do not only type prompts; they paste. A prompt that carries the file
//! it is about is a *better* prompt than one that does not, and the score has
//! to say so.
//!
//! It did not. Attaching the source file a task was about cost 70 points out
//! of 1000, almost all of it in grounding: the file contributed 434 further
//! names to a weighted average built for the dozen a person writes by hand,
//! and the handful that actually pinned the work down were drowned by
//! `self`, `let` and `usize`.
//!
//! The mistake was treating everything in the box as something the user
//! *said*. An attachment is not a claim, it is evidence. It can hand the agent
//! a name it needed; it cannot make the request vaguer. So the two are
//! separated, and attachments are allowed to raise grounding and never to
//! lower it.
//!
//! # How the split is made
//!
//! Through the API the caller says which is which. From a raw blob -- what the
//! hook and the command line see -- a fenced block that is long enough to be
//! pasted rather than typed is taken as an attachment. Short spans stay inline:
//! `verify_token` in the middle of a sentence is something the user wrote.

use yp_lang::Span;

use crate::params::prompt as p;

/// A prompt, separated into what was written and what was attached.
#[derive(Debug, Clone)]
pub struct Parts<'a> {
    /// The prose the user composed, with attachments cut out.
    pub instruction: String,
    /// Pasted or uploaded material, in the order it appeared.
    pub attachments: Vec<&'a str>,
    /// Where each attachment marker sits in `instruction`, and the bytes of
    /// the original text it stands in for. Lets a span found in the
    /// instruction be mapped back onto what the user actually has in front
    /// of them.
    cuts: Vec<Cut>,
    /// Length of the text the instruction was cut from.
    original_len: usize,
}

/// One place where the instruction departs from the original text.
#[derive(Debug, Clone, Copy)]
struct Cut {
    /// Byte offset in `instruction` where the marker begins.
    at: usize,
    /// The region of the original text the marker replaces.
    original: Span,
}

impl Parts<'_> {
    /// Everything the user typed, judged as an instruction.
    pub fn instruction(&self) -> &str {
        &self.instruction
    }

    pub fn has_attachments(&self) -> bool {
        !self.attachments.is_empty()
    }

    /// Map a byte range of the instruction back onto the original text.
    ///
    /// `None` when the range touches an attachment marker, which stands for
    /// nothing the user wrote, or falls past the end of the original.
    pub fn to_original(&self, span: Span) -> Option<Span> {
        let marker_len = p::ATTACHMENT_MARKER.len();
        let mut shift: isize = 0;
        for cut in &self.cuts {
            let marker = Span::new(cut.at, cut.at + marker_len);
            if span.start >= marker.end {
                shift += cut.original.len() as isize - marker_len as isize;
                continue;
            }
            if span.overlaps(&marker) {
                return None;
            }
            break;
        }
        let start = span.start as isize + shift;
        let end = span.end as isize + shift;
        if start < 0 || end > self.original_len as isize {
            return None;
        }
        Some(Span::new(start as usize, end as usize))
    }

    /// The byte ranges of the original text that were taken as attachments.
    pub fn attachment_spans(&self) -> Vec<Span> {
        self.cuts
            .iter()
            .map(|c| c.original)
            .filter(|s| !s.is_empty())
            .collect()
    }
}

/// The content of a fenced region, without the fence or its language tag.
///
/// An attachment is the material, not the markup around it. Leaving the
/// backticks on meant the whole file was later read back as a single code
/// span and looked up as if it were one enormous identifier, so no attachment
/// ever contributed an anchor.
fn strip_fence(body: &str) -> &str {
    let inner = body.trim_matches('`');
    // The opening line of a fence is often a bare language tag, which is
    // markup too.
    match inner.split_once('\n') {
        Some((first, rest))
            if !first.trim().is_empty()
                && first
                    .trim()
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-') =>
        {
            rest
        }
        _ => inner,
    }
}

/// A fenced region long enough to have been pasted rather than typed.
fn is_attachment(text: &str) -> bool {
    let inner = text.trim_matches('`').trim();
    inner.len() >= p::ATTACHMENT_MIN_BYTES || inner.lines().count() >= p::ATTACHMENT_MIN_LINES
}

/// Split a raw prompt into instruction and attachments.
///
/// The instruction keeps a marker where each attachment was removed, so that
/// the surrounding sentence still reads as a sentence and the code-evidence
/// signal is not lost along with the code.
pub fn split(text: &str) -> Parts<'_> {
    let regions: Vec<Span> = yp_lang::token::code_regions(text);
    let mut attachments = Vec::new();
    let mut cuts = Vec::new();
    let mut instruction = String::with_capacity(text.len());
    let mut cursor = 0usize;

    for region in regions {
        let body = region.slice(text);
        if !is_attachment(body) {
            continue;
        }
        instruction.push_str(&text[cursor..region.start]);
        // A short stand-in, so the prompt still contains code evidence and the
        // sentence around the paste does not run into itself.
        cuts.push(Cut {
            at: instruction.len(),
            original: region,
        });
        instruction.push_str(p::ATTACHMENT_MARKER);
        cursor = region.end;
        attachments.push(strip_fence(body));
    }
    instruction.push_str(&text[cursor..]);

    Parts {
        instruction,
        attachments,
        cuts,
        original_len: text.len(),
    }
}

/// Build parts from an instruction and attachments the caller already has
/// separated, as the web page does.
pub fn from_parts<'a>(instruction: &str, attachments: &[&'a str]) -> Parts<'a> {
    let mut text = instruction.to_string();
    let mut cuts = Vec::new();
    if !attachments.is_empty() {
        // The instruction still counts as carrying evidence. The marker
        // stands for nothing in the caller's text, so it maps to an empty
        // region at its end.
        text.push(' ');
        cuts.push(Cut {
            at: text.len(),
            original: Span::new(instruction.len(), instruction.len()),
        });
        text.push_str(p::ATTACHMENT_MARKER);
    }
    Parts {
        instruction: text,
        attachments: attachments.to_vec(),
        cuts,
        original_len: instruction.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_span_stays_part_of_the_sentence() {
        let parts = split("fix `verify_token` in src/auth.rs");
        assert!(parts.attachments.is_empty());
        assert!(parts.instruction.contains("verify_token"));
    }

    #[test]
    fn an_attachment_is_the_material_without_the_markup() {
        let body = (0..10)
            .map(|i| format!("let value_{i} = verify_token();\n"))
            .collect::<String>();
        let text = format!("fix it\n\n```rust\n{body}```");
        let parts = split(&text);
        assert_eq!(parts.attachments.len(), 1);
        let body = parts.attachments[0];
        assert!(!body.contains('`'), "fence survived: {body:?}");
        assert!(!body.starts_with("rust"), "language tag survived: {body:?}");
        assert!(body.contains("verify_token"));
    }

    #[test]
    fn a_pasted_file_becomes_an_attachment() {
        let file: String = (0..40)
            .map(|i| format!("fn generated_{i}() {{ let value = {i}; }}\n"))
            .collect();
        let text = format!("fix the parser.\n\n```rust\n{file}```\n");
        let parts = split(&text);

        assert_eq!(parts.attachments.len(), 1);
        assert!(parts.attachments[0].contains("generated_7"));
        assert!(!parts.instruction.contains("generated_7"));
        assert!(parts.instruction.contains("fix the parser"));
    }

    #[test]
    fn the_instruction_still_shows_that_code_was_attached() {
        let file = "x\n".repeat(40);
        let text = format!("fix it\n\n```\n{file}```");
        let parts = split(&text);
        // The marker keeps the code-evidence signal without the code.
        assert!(parts.instruction.contains('`'));
        assert!(parts.instruction.len() < 80, "got {:?}", parts.instruction);
    }

    #[test]
    fn several_attachments_are_all_kept_in_order() {
        let block = |n: usize| format!("```\n{}```", format!("line {n}\n").repeat(30));
        let text = format!("compare {} and then {}", block(1), block(2));
        let parts = split(&text);
        assert_eq!(parts.attachments.len(), 2);
        assert!(parts.attachments[0].contains("line 1"));
        assert!(parts.attachments[1].contains("line 2"));
        assert!(parts.instruction.contains("compare"));
        assert!(parts.instruction.contains("and then"));
    }

    #[test]
    fn a_prompt_with_no_code_is_unchanged() {
        let text = "rename parse_args to parse_arguments in src/cli.rs";
        let parts = split(text);
        assert!(parts.attachments.is_empty());
        assert_eq!(parts.instruction, text);
    }

    #[test]
    fn explicit_parts_are_taken_as_given() {
        let file = "whatever the user uploaded";
        let parts = from_parts("fix the parser", &[file]);
        assert_eq!(parts.attachments, vec![file]);
        assert!(parts.instruction.starts_with("fix the parser"));
        assert!(parts.has_attachments());
    }

    #[test]
    fn explicit_parts_without_attachments_leave_the_instruction_alone() {
        let parts = from_parts("fix the parser", &[]);
        assert_eq!(parts.instruction, "fix the parser");
        assert!(!parts.has_attachments());
    }

    #[test]
    fn an_unterminated_fence_does_not_swallow_the_instruction_silently() {
        // `code_regions` runs an unterminated fence to the end of the text, so
        // everything after it is attachment. The instruction before it must
        // survive.
        let text = format!("fix the parser\n\n```rust\n{}", "line\n".repeat(40));
        let parts = split(&text);
        assert!(parts.instruction.contains("fix the parser"));
        assert_eq!(parts.attachments.len(), 1);
    }
}
