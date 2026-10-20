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

/// How much of an instruction is scored: its first 64 KB.
///
/// The hook runs before every prompt is sent, and Claude Code cancels a hook
/// that takes too long; the plugin gives it five seconds. Some passes still
/// grow faster than the text they read. With django's index loaded, a
/// megabyte of "why does it fail?" took the hook 114.7 s and a megabyte of
/// short fenced blocks 26.4 s. With the question, negation and tokenizer
/// passes made linear, they still took 26.0 s and 5.2 s. Read to here, they
/// take 80 ms and 42 ms, and 502 ms and 103 ms without those fixes.
///
/// No benchmark prompt comes near it. The longest HumanEvalComm prompt is
/// 1,360 bytes. Once pasted material is cut out, the longest instruction
/// among the 270 SWE-bench issues scored is 6,768 bytes, and the longest
/// issue, pytest-dev__pytest-7490 at 22,778 bytes, is mostly pasted console
/// output.
pub const MAX_INSTRUCTION_BYTES: usize = 64 * 1024;

/// How much attached material is read, across all attachments: its first
/// 64 KB.
///
/// Material is judged on the share of its names the repository recognises,
/// a proportion its first 64 KB estimate. Reading all of it is not ruinous --
/// a 16 MB pasted log took the hook 70 ms against django's index, and takes
/// 60 ms read to here -- but what a hook costs should not grow with whatever
/// was pasted into it. The most any SWE-bench issue attaches is 22,136
/// bytes.
pub const MAX_ATTACHED_BYTES: usize = 64 * 1024;

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
    /// The text the instruction was cut from.
    original: &'a str,
}

/// One place where the instruction departs from the original text.
#[derive(Debug, Clone, Copy)]
struct Cut {
    /// Byte offset in `instruction` where the marker begins.
    at: usize,
    /// The region of the original text the marker replaces.
    original: Span,
}

impl<'a> Parts<'a> {
    /// Everything the user typed, judged as an instruction.
    pub fn instruction(&self) -> &str {
        &self.instruction
    }

    pub fn has_attachments(&self) -> bool {
        !self.attachments.is_empty()
    }

    /// The text these parts were cut from, as it was sent: the raw prompt,
    /// or what was typed beside the files a caller attached on their own.
    ///
    /// Whether Claude Code sent a turn is read here as well as in the
    /// instruction. A stray backtick runs a code region to the end of the
    /// text, and cut out as pasted it took the close tag that marks a report
    /// as Claude Code's with it: a background agent's report that mentioned
    /// "```" was graded 569.1, C+, in place of the task before it.
    pub fn original(&self) -> &'a str {
        self.original
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
        if start < 0 || end > self.original.len() as isize {
            return None;
        }
        Some(Span::new(start as usize, end as usize))
    }

    /// These parts as the scorer reads them: the first
    /// [`MAX_INSTRUCTION_BYTES`] of the instruction and the first
    /// [`MAX_ATTACHED_BYTES`] of attached material, each cut on a character
    /// boundary. `None` when everything already fits.
    ///
    /// Spans found in the shortened instruction map back onto the original
    /// text as before; an attachment marker is kept whole or dropped.
    pub fn capped(&self) -> Option<Parts<'a>> {
        let attached: usize = self.attachments.iter().map(|a| a.len()).sum();
        if self.instruction.len() <= MAX_INSTRUCTION_BYTES && attached <= MAX_ATTACHED_BYTES {
            return None;
        }
        let marker = p::ATTACHMENT_MARKER.len();
        let mut end = self.instruction.len().min(MAX_INSTRUCTION_BYTES);
        while !self.instruction.is_char_boundary(end) {
            end -= 1;
        }
        if let Some(cut) = self.cuts.iter().find(|c| c.at < end && end < c.at + marker) {
            end = cut.at;
        }
        Some(Parts {
            instruction: self.instruction[..end].to_string(),
            attachments: heads(&self.attachments, MAX_ATTACHED_BYTES),
            cuts: self
                .cuts
                .iter()
                .copied()
                .filter(|c| c.at + marker <= end)
                .collect(),
            original: self.original,
        })
    }

    /// The byte ranges of the original text that were taken as attachments.
    pub fn attachment_spans(&self) -> Vec<Span> {
        self.cuts
            .iter()
            .map(|c| c.original)
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Everything that was pasted rather than typed, without its fences:
    /// the attachments, and any fenced block still inline in the instruction.
    ///
    /// A fence too short for [`split`] to cut out is pasted material all the
    /// same.
    pub fn pasted(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self.attachments.clone();
        for region in yp_lang::token::code_regions(&self.instruction) {
            let body = region.slice(&self.instruction);
            if is_pasted_block(body) {
                out.push(strip_fence(body));
            }
        }
        out
    }
}

/// A code region that was pasted rather than typed: opened by a fence of
/// three or more backticks, or long enough that [`split`] takes it as an
/// attachment. An inline span like `verify_token` is something the user
/// wrote.
pub(crate) fn is_pasted_block(body: &str) -> bool {
    body.starts_with("```") || is_attachment(body)
}

/// The content of a code region, without the backticks or a fence's info
/// string.
///
/// An attachment is the material, not the markup around it. Leaving the
/// backticks on meant the whole file was later read back as a single code
/// span and looked up as if it were one enormous identifier, so no attachment
/// ever contributed an anchor.
///
/// Whatever follows the opening backticks of a fence on its first line is
/// the info string -- `rust`, `python title="app.py"`, `c++` -- and markup
/// too. Read as code, it was looked up as a name: pasting the demo's own
/// `login.rs` in a `rust` fence reported "rust" as not in the repository.
/// Only a fence has one. A span opened by fewer than three backticks, or one
/// that never leaves its line, is all content.
pub(crate) fn strip_fence(body: &str) -> &str {
    let inner = body.trim_matches('`');
    if body.starts_with("```") {
        if let Some((info, rest)) = inner.split_once('\n') {
            if !info.trim().is_empty() {
                return rest;
            }
        }
    }
    inner
}

/// The blocks in order until together they hold `budget` bytes, the last one
/// cut on a character boundary: the attachments a capped prompt keeps, and
/// the pasted material that is read to judge it.
pub(crate) fn heads<'a>(blocks: &[&'a str], budget: usize) -> Vec<&'a str> {
    let mut left = budget;
    let mut out = Vec::new();
    for &block in blocks {
        if block.len() <= left {
            out.push(block);
            left -= block.len();
            continue;
        }
        let mut end = left;
        while !block.is_char_boundary(end) {
            end -= 1;
        }
        if end > 0 {
            out.push(&block[..end]);
        }
        break;
    }
    out
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
        original: text,
    }
}

/// Build parts from what the user typed and the files the caller attached
/// separately, as the web page does.
///
/// The typed text is split exactly as [`split`] splits a raw prompt, so a
/// fence pasted into the page's box is cut out the way the hook cuts it out
/// of the same prompt. Kept inline, a pasted file's names were read as names
/// the user had typed: with the demo's sample repository, the showcase prompt
/// with the sample's `login.rs` pasted under it scored 832.0 on the page and
/// 845.7 in the status line.
pub fn from_parts<'a>(instruction: &'a str, attachments: &[&'a str]) -> Parts<'a> {
    let mut parts = split(instruction);
    if !attachments.is_empty() {
        // The instruction still counts as carrying evidence. The marker
        // stands for nothing in the caller's text, so it maps to an empty
        // region at its end.
        parts.instruction.push(' ');
        parts.cuts.push(Cut {
            at: parts.instruction.len(),
            original: Span::new(instruction.len(), instruction.len()),
        });
        parts.instruction.push_str(p::ATTACHMENT_MARKER);
        parts.attachments.extend_from_slice(attachments);
    }
    parts
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

    #[test]
    fn what_was_typed_beside_an_opened_file_is_split_like_a_raw_prompt() {
        let paste = "let value = verify_token();\n".repeat(20);
        let typed = format!("fix the parser\n```rust\n{paste}```\nroughly");
        let file = "whatever the user uploaded";
        let parts = from_parts(&typed, &[file]);
        assert_eq!(parts.attachments, vec![paste.as_str(), file]);
        assert_eq!(parts.instruction.matches(p::ATTACHMENT_MARKER).count(), 2);

        // Spans still land on what was typed: the paste, then the word after it.
        let marked = parts.attachment_spans();
        assert_eq!(marked.len(), 1);
        assert!(typed[marked[0].start..marked[0].end].starts_with("```rust"));
        let at = parts.instruction.find("roughly").unwrap();
        let back = parts.to_original(Span::new(at, at + 7)).unwrap();
        assert_eq!(&typed[back.start..back.end], "roughly");
    }

    #[test]
    fn capped_parts_still_map_onto_the_original() {
        let paste = "line of the paste\n".repeat(40);
        let prose = "the parser should accept it. ".repeat(4000);
        let text = format!("fix the parser\n```\n{paste}```\n{prose}");
        let parts = split(&text);
        assert!(parts.instruction.len() > MAX_INSTRUCTION_BYTES);

        let capped = parts.capped().expect("over the limit");
        assert_eq!(capped.instruction.len(), MAX_INSTRUCTION_BYTES);
        assert_eq!(capped.attachments, parts.attachments);
        assert_eq!(capped.attachment_spans(), parts.attachment_spans());
        let at = capped.instruction.rfind("accept").unwrap();
        let back = capped.to_original(Span::new(at, at + 6)).unwrap();
        assert_eq!(&text[back.start..back.end], "accept");

        // Attached material is read to its own limit, on a character boundary.
        let big = "가".repeat(MAX_ATTACHED_BYTES);
        let parts = from_parts("fix it", &[big.as_str(), "never read"]);
        let capped = parts.capped().expect("over the limit");
        assert_eq!(capped.attachments.len(), 1);
        assert!(capped.attachments[0].len() <= MAX_ATTACHED_BYTES);
        assert!(big.starts_with(capped.attachments[0]));
        assert_eq!(capped.instruction, parts.instruction);
    }

    #[test]
    fn the_parts_keep_the_text_they_were_cut_from() {
        let paste = "line of the paste\n".repeat(40);
        let text = format!("fix the parser\n```\n{paste}```\n");
        let parts = split(&text);
        assert!(!parts.instruction().contains("line of the paste"));
        assert_eq!(parts.original(), text);
        // What was typed, without the files opened beside it.
        let opened = from_parts("fix the parser", &["whatever the user uploaded"]);
        assert_eq!(opened.original(), "fix the parser");
        // Read to the limit, they still hold all of it.
        let long = "the parser should accept it. ".repeat(4000);
        assert_eq!(split(&long).capped().unwrap().original(), long);
    }

    #[test]
    fn only_a_fence_has_an_info_string() {
        assert_eq!(strip_fence("```rust\nfn x()\n```"), "fn x()\n");
        assert_eq!(
            strip_fence("``` python title=\"a.py\"\nx = 1\n```"),
            "x = 1\n"
        );
        assert_eq!(strip_fence("```c++\r\nint x;\r\n```"), "int x;\r\n");
        // No info string: the opening line is empty.
        assert_eq!(strip_fence("```\nverify_token\n```"), "\nverify_token\n");
        // An inline span, however it is opened, is all content.
        assert_eq!(strip_fence("```verify_token```"), "verify_token");
        assert_eq!(strip_fence("`login\nhandler`"), "login\nhandler");
    }
}
