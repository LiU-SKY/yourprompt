/// A byte range into the original prompt text.
///
/// Byte offsets, not char offsets: prompts are UTF-8 and Korean text makes
/// char indexing a trap. Every span produced by this crate is guaranteed to
/// fall on a UTF-8 character boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        debug_assert!(start <= end, "span start must not exceed end");
        Self { start, end }
    }

    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// True when the two ranges share at least one byte.
    pub fn overlaps(&self, other: &Span) -> bool {
        self.start < other.end && other.start < self.end
    }

    pub fn contains(&self, offset: usize) -> bool {
        offset >= self.start && offset < self.end
    }

    pub fn slice<'t>(&self, text: &'t str) -> &'t str {
        &text[self.start..self.end]
    }
}

/// True when `offset` falls inside any span in a *sorted, non-overlapping* list.
///
/// Used to skip regions the caller has masked out, such as fenced code blocks.
pub fn covered_by(spans: &[Span], offset: usize) -> bool {
    spans
        .binary_search_by(|s| {
            if offset < s.start {
                std::cmp::Ordering::Greater
            } else if offset >= s.end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}
