//! Line and column structure for one source's text (docs/design/
//! base.md §5): an explicit, pure derivation you construct and hold.
//! Zero-based throughout; one-based coordinates exist only inside the
//! human rendering, as presentation. Lines break at `\n` alone; a
//! `\r` stays in its line's content; nothing is normalized, ever.

use std::fmt;

use crate::source::{NotCharBoundary, Source};
use crate::span::{ByteOffset, Span};

/// A zero-based line/column coordinate. What `col` counts is named by
/// the encoding the query stated; the coordinate is a transient query
/// result and deliberately does not carry it (base.md §5). Fields are
/// public: any pair is a valid coordinate value; validity against a
/// text is checked at use.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct LineCol {
    /// Zero-based line.
    pub line: u32,
    /// Zero-based column, in the units the query's encoding named.
    pub col: u32,
}

/// What a column counts. UTF-16 units exist because the editor
/// protocol's default position encoding demands them (base.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ColumnEncoding {
    /// Bytes of UTF-8.
    Utf8Bytes,
    /// Unicode code points.
    CodePoints,
    /// UTF-16 code units.
    Utf16Units,
}

/// An offset strictly past the end-of-text position (base.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct OffsetOutOfBounds {
    /// The offending offset.
    pub offset: ByteOffset,
    /// The end-of-text position — the largest in-bounds offset.
    pub max: ByteOffset,
}

// The boundary condition `NotCharBoundary` is base.md §3.2's shared
// struct, defined in the source module and wrapped here by
// `PositionRefusal`.

/// A line at or past the line count (base.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LineOutOfBounds {
    /// The offending line.
    pub line: u32,
    /// How many lines the text has.
    pub line_count: u32,
}

/// A column past its line's extent; `max` is the extent in the
/// encoding the query stated (base.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ColumnOutOfBounds {
    /// The line queried.
    pub line: u32,
    /// The offending column.
    pub col: u32,
    /// The line's extent in the stated encoding.
    pub max: u32,
}

/// A column inside a multi-unit character in the stated encoding
/// (base.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ColumnNotBoundary {
    /// The line queried.
    pub line: u32,
    /// The offending column.
    pub col: u32,
}

/// What `LineIndex::position` can refuse (base.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PositionRefusal {
    /// The offset is past the end of the text.
    OutOfBounds(OffsetOutOfBounds),
    /// The offset is inside a multi-byte character.
    NotCharBoundary(NotCharBoundary),
}

/// What `LineIndex::offset` can refuse (base.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OffsetRefusal {
    /// The line is past the line count.
    LineOutOfBounds(LineOutOfBounds),
    /// The column is past the line's extent.
    ColumnOutOfBounds(ColumnOutOfBounds),
    /// The column is inside a multi-unit character.
    ColumnNotBoundary(ColumnNotBoundary),
}

impl fmt::Display for OffsetOutOfBounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "byte {} is past the text's end at byte {}",
            self.offset.get(),
            self.max.get()
        )
    }
}

impl std::error::Error for OffsetOutOfBounds {}

impl fmt::Display for LineOutOfBounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "line {} is out of bounds; the text has {} line(s)",
            self.line, self.line_count
        )
    }
}

impl std::error::Error for LineOutOfBounds {}

impl fmt::Display for ColumnOutOfBounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "column {} is past line {}'s extent {}",
            self.col, self.line, self.max
        )
    }
}

impl std::error::Error for ColumnOutOfBounds {}

impl fmt::Display for ColumnNotBoundary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "column {} on line {} is not a character boundary",
            self.col, self.line
        )
    }
}

impl std::error::Error for ColumnNotBoundary {}

impl fmt::Display for PositionRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PositionRefusal::OutOfBounds(refusal) => refusal.fmt(f),
            PositionRefusal::NotCharBoundary(refusal) => refusal.fmt(f),
        }
    }
}

impl std::error::Error for PositionRefusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PositionRefusal::OutOfBounds(refusal) => Some(refusal),
            PositionRefusal::NotCharBoundary(refusal) => Some(refusal),
        }
    }
}

impl fmt::Display for OffsetRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OffsetRefusal::LineOutOfBounds(refusal) => refusal.fmt(f),
            OffsetRefusal::ColumnOutOfBounds(refusal) => refusal.fmt(f),
            OffsetRefusal::ColumnNotBoundary(refusal) => refusal.fmt(f),
        }
    }
}

impl std::error::Error for OffsetRefusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            OffsetRefusal::LineOutOfBounds(refusal) => Some(refusal),
            OffsetRefusal::ColumnOutOfBounds(refusal) => Some(refusal),
            OffsetRefusal::ColumnNotBoundary(refusal) => Some(refusal),
        }
    }
}

/// One non-ASCII character: where it starts and what it costs in each
/// encoding. With the running surpluses beside it, this is enough to
/// answer all three encodings — and to *refuse* a mid-character offset
/// rather than misplace a caret — without retaining the text
/// (base.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct WideChar {
    /// Byte offset of the character's first byte.
    offset: u32,
    /// 2..=4 bytes of UTF-8.
    byte_len: u8,
    /// 1 or 2 UTF-16 units.
    utf16_len: u8,
}

/// Line and column structure for one source's text: an explicit, pure
/// derivation you construct and hold. Does not retain the text
/// (base.md §5). Memory is O(lines + non-ASCII characters).
///
/// A named refinement of the design's sketch: base.md §5 pictures the
/// non-ASCII characters grouped per line; this index holds one
/// offset-sorted table with prefix-sum surpluses instead — the same
/// data and the same memory bound with simpler structure, and the
/// query bound is O(log lines + log non-ASCII in the text).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LineIndex {
    /// Byte offset where each line starts; entry 0 is always 0, and a
    /// new entry follows every `\n`.
    line_starts: Vec<u32>,
    /// Every non-ASCII character, in offset order.
    wide: Vec<WideChar>,
    /// Running total over `wide[..=i]` of `byte_len - 1`: bytes minus
    /// code points contributed by wide characters. Prefix sums keep
    /// column queries logarithmic (base.md §5's stated cost).
    cp_surplus: Vec<u32>,
    /// Running total over `wide[..=i]` of `byte_len - utf16_len`:
    /// bytes minus UTF-16 units, likewise.
    utf16_surplus: Vec<u32>,
    /// The end-of-text position, in bytes.
    len: u32,
}

impl LineIndex {
    /// Indexes one admitted source. Total: every admitted `Source`
    /// indexes — riding on `Source` admission keeps one authority for
    /// text; there is no `&str` door (base.md §5). O(n) in the text.
    pub fn of(source: &Source) -> LineIndex {
        let text = source.text();
        let mut line_starts = vec![0u32];
        let mut wide = Vec::new();
        let mut cp_surplus = Vec::new();
        let mut utf16_surplus = Vec::new();
        let mut cp_running = 0u32;
        let mut utf16_running = 0u32;
        for (offset, character) in text.char_indices() {
            // The cast cannot truncate: admission guards the length.
            let offset = offset as u32;
            if character == '\n' {
                line_starts.push(offset + 1);
            }
            if !character.is_ascii() {
                let byte_len = character.len_utf8() as u8;
                let utf16_len = character.len_utf16() as u8;
                wide.push(WideChar {
                    offset,
                    byte_len,
                    utf16_len,
                });
                cp_running += u32::from(byte_len) - 1;
                utf16_running += u32::from(byte_len) - u32::from(utf16_len);
                cp_surplus.push(cp_running);
                utf16_surplus.push(utf16_running);
            }
        }
        LineIndex {
            line_starts,
            wide,
            cp_surplus,
            utf16_surplus,
            len: source.end().get(),
        }
    }

    /// How many lines the text has; empty text has one. Total; O(1) —
    /// a stored count (base.md §5, §9).
    pub fn line_count(&self) -> u32 {
        // The cast cannot truncate: there are at most len + 1 lines,
        // and the admission ceiling guards len + 1 <= u32::MAX
        // (base.md §3.2).
        self.line_starts.len() as u32
    }

    /// The span of one line's content, excluding its terminator —
    /// renderers want the content; the terminator is derivable.
    /// Refuses `LineOutOfBounds`; O(1) — a bounds check and an array
    /// lookup; the terminator exclusion is deterministic from adjacent
    /// line starts, because lines break only at `\n` (base.md §5).
    pub fn line_span(&self, line: u32) -> Result<Span, LineOutOfBounds> {
        let line_count = self.line_count();
        if line >= line_count {
            return Err(LineOutOfBounds { line, line_count });
        }
        let start = self.line_starts[line as usize];
        let end = match self.line_starts.get(line as usize + 1) {
            // The byte before the next line's start is this line's
            // terminating '\n'.
            Some(next_start) => next_start - 1,
            None => self.len,
        };
        // Total construction of an ordered region: join of two empty
        // spans, as in Source::span.
        Ok(Span::empty(ByteOffset::new(start)).join(Span::empty(ByteOffset::new(end))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{Source, SourceId};
    use crate::span::ByteOffset;

    fn index_of(text: &str) -> LineIndex {
        let source = Source::new(SourceId::new(0), text.to_owned()).expect("test text admits");
        LineIndex::of(&source)
    }

    #[test]
    fn empty_text_has_one_empty_line() {
        let index = index_of("");
        assert_eq!(index.line_count(), 1);
        let span = index.line_span(0).expect("line 0 exists");
        assert_eq!(span.start(), ByteOffset::ZERO);
        assert_eq!(span.end(), ByteOffset::ZERO);
    }

    #[test]
    fn lines_break_at_newline() {
        let index = index_of("ab\ncd");
        assert_eq!(index.line_count(), 2);
        let second = index.line_span(1).unwrap();
        assert_eq!((second.start().get(), second.end().get()), (3, 5));
    }

    #[test]
    fn content_excludes_the_terminator() {
        let index = index_of("ab\ncd");
        let first = index.line_span(0).unwrap();
        assert_eq!((first.start().get(), first.end().get()), (0, 2));
    }

    #[test]
    fn a_trailing_newline_yields_a_final_empty_line() {
        let index = index_of("ab\n");
        assert_eq!(index.line_count(), 2);
        let last = index.line_span(1).unwrap();
        assert_eq!((last.start().get(), last.end().get()), (3, 3));
    }

    #[test]
    fn carriage_return_stays_in_its_line_content() {
        // CRLF: the '\r' is content, only '\n' terminates
        // (base.md §5, the rust-analyzer convention).
        let index = index_of("ab\r\ncd");
        let first = index.line_span(0).unwrap();
        assert_eq!((first.start().get(), first.end().get()), (0, 3));
    }

    #[test]
    fn a_lone_carriage_return_is_not_a_line_break() {
        let index = index_of("ab\rcd");
        assert_eq!(index.line_count(), 1);
    }

    #[test]
    fn line_span_refuses_out_of_bounds_with_the_count() {
        let index = index_of("ab");
        assert_eq!(
            index.line_span(3),
            Err(LineOutOfBounds {
                line: 3,
                line_count: 1
            })
        );
    }

    #[test]
    fn refusals_display_the_fixable_question() {
        assert_eq!(
            LineOutOfBounds {
                line: 3,
                line_count: 1
            }
            .to_string(),
            "line 3 is out of bounds; the text has 1 line(s)"
        );
        assert_eq!(
            ColumnOutOfBounds {
                line: 2,
                col: 9,
                max: 4
            }
            .to_string(),
            "column 9 is past line 2's extent 4"
        );
        assert_eq!(
            ColumnNotBoundary { line: 2, col: 3 }.to_string(),
            "column 3 on line 2 is not a character boundary"
        );
        assert_eq!(
            OffsetOutOfBounds {
                offset: ByteOffset::new(9),
                max: ByteOffset::new(4),
            }
            .to_string(),
            "byte 9 is past the text's end at byte 4"
        );
        assert_eq!(
            NotCharBoundary {
                offset: ByteOffset::new(2)
            }
            .to_string(),
            "byte 2 is not a character boundary"
        );
        let _: &dyn std::error::Error = &LineOutOfBounds {
            line: 0,
            line_count: 1,
        };
    }
}
