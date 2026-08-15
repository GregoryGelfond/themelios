//! Byte positions and half-open regions in one source's text
//! (docs/design/base.md §4). A `Span` is text-independent arithmetic
//! data; boundary discipline lives where span meets text — see
//! `Source::slice` and the line index.

use std::fmt;

/// A position in a source's UTF-8 text, in bytes. The unit is in the
/// type's name so it is never in a comment (base.md §4.1).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ByteOffset(u32);

impl ByteOffset {
    /// The zero offset: the start of any text.
    pub const ZERO: ByteOffset = ByteOffset(0);

    /// Wraps a raw byte count. Total; O(1).
    pub const fn new(raw: u32) -> ByteOffset {
        ByteOffset(raw)
    }

    /// The raw byte count. Total; O(1).
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Checked arithmetic only: overflow answers `None`, never wraps
    /// (base.md §4.1). O(1).
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub const fn checked_add(self, bytes: u32) -> Option<ByteOffset> {
        match self.0.checked_add(bytes) {
            Some(raw) => Some(ByteOffset(raw)),
            None => None,
        }
    }

    /// Checked arithmetic only: underflow answers `None`, never wraps
    /// (base.md §4.1). O(1).
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub const fn checked_sub(self, bytes: u32) -> Option<ByteOffset> {
        match self.0.checked_sub(bytes) {
            Some(raw) => Some(ByteOffset(raw)),
            None => None,
        }
    }
}

/// A half-open byte region `[start, end)` in one source's text
/// (base.md §4.2). The one guarded invariant is `start <= end`;
/// derived ordering is (start, end) — document order with
/// shorter-first ties.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Span {
    start: ByteOffset,
    end: ByteOffset,
}

/// The one refusal `Span` construction can issue, carried as the
/// condition itself (base.md §4.2, §3.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EndBeforeStart {
    /// The offered start.
    pub start: ByteOffset,
    /// The offered end, strictly before the start.
    pub end: ByteOffset,
}

impl fmt::Display for EndBeforeStart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "span end {} is before its start {}",
            self.end.get(),
            self.start.get()
        )
    }
}

impl std::error::Error for EndBeforeStart {}

impl Span {
    /// Refuses `EndBeforeStart`; O(1).
    pub fn new(start: ByteOffset, end: ByteOffset) -> Result<Span, EndBeforeStart> {
        if end < start {
            Err(EndBeforeStart { start, end })
        } else {
            Ok(Span { start, end })
        }
    }

    /// The empty span at one position. Total; O(1).
    pub const fn empty(at: ByteOffset) -> Span {
        Span { start: at, end: at }
    }

    /// The covering span of two offsets, `[min, max)` — ordered by
    /// construction, so the crate builds regions it knows to be ordered
    /// (a text's extent, a line's content) without a fallible door.
    /// Crate-private: the public surface stays exactly base.md §4.2's.
    /// Total; O(1).
    pub(crate) const fn covering(a: ByteOffset, b: ByteOffset) -> Span {
        if a.0 <= b.0 {
            Span { start: a, end: b }
        } else {
            Span { start: b, end: a }
        }
    }

    /// The start offset. Total; O(1).
    pub fn start(self) -> ByteOffset {
        self.start
    }

    /// The one-past-end offset. Total; O(1).
    pub fn end(self) -> ByteOffset {
        self.end
    }

    /// Length in bytes. Total; O(1).
    pub fn len(self) -> u32 {
        // Cannot underflow: start <= end is guarded at construction.
        self.end.get() - self.start.get()
    }

    /// Whether the region is empty. Total; O(1).
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Whether `offset` lies inside the half-open region. Total; O(1).
    pub fn contains(self, offset: ByteOffset) -> bool {
        self.start <= offset && offset < self.end
    }

    /// Whether `other` lies entirely within `self`. Total; O(1).
    pub fn contains_span(self, other: Span) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    /// Interval intersection: `Some` exactly when the intervals meet,
    /// including an empty span at a touch point — which keeps this
    /// consistent with `contains_span` on empty operands. Total; O(1).
    /// Dropping the result loses the intersection, so the by-value
    /// combinator is must-use.
    #[must_use]
    pub fn intersect(self, other: Span) -> Option<Span> {
        let start = self.start.max(other.start);
        let end = self.end.min(other.end);
        if start <= end {
            Some(Span { start, end })
        } else {
            None
        }
    }

    /// The covering span — total, including disjoint operands
    /// (base.md §4.2). O(1). Dropping the result loses the join, so
    /// the by-value combinator is must-use.
    #[must_use]
    pub fn join(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// A span in a named source — the cross-source form (base.md §4.3).
/// Fields are public: any (source, span) pair is a valid value;
/// validity against a particular text is checked where text is in
/// scope. Derived ordering is (source, then span): batch order groups
/// by source.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Location {
    /// The source the span points into.
    pub source: crate::source::SourceId,
    /// The region within that source's text.
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;

    #[test]
    fn construction_refuses_end_before_start() {
        let refusal = Span::new(ByteOffset::new(5), ByteOffset::new(3));
        assert_eq!(
            refusal,
            Err(EndBeforeStart {
                start: ByteOffset::new(5),
                end: ByteOffset::new(3),
            })
        );
    }

    #[test]
    fn empty_span_has_no_extent_and_contains_nothing() {
        let span = Span::empty(ByteOffset::new(7));
        assert_eq!(span.len(), 0);
        assert!(span.is_empty());
        assert!(!span.contains(ByteOffset::new(7)));
    }

    #[test]
    fn contains_is_half_open() {
        let span = Span::new(ByteOffset::new(2), ByteOffset::new(5)).expect("ordered endpoints");
        assert!(span.contains(ByteOffset::new(2)));
        assert!(span.contains(ByteOffset::new(4)));
        assert!(!span.contains(ByteOffset::new(5)));
    }

    #[test]
    fn join_is_total_including_disjoint_operands() {
        let a = Span::new(ByteOffset::new(0), ByteOffset::new(2)).unwrap();
        let b = Span::new(ByteOffset::new(6), ByteOffset::new(9)).unwrap();
        let joined = a.join(b);
        assert_eq!(joined.start(), ByteOffset::new(0));
        assert_eq!(joined.end(), ByteOffset::new(9));
    }

    #[test]
    fn intersect_is_interval_intersection() {
        let a = Span::new(ByteOffset::new(0), ByteOffset::new(4)).unwrap();
        let b = Span::new(ByteOffset::new(2), ByteOffset::new(6)).unwrap();
        let c = Span::new(ByteOffset::new(4), ByteOffset::new(6)).unwrap();
        let d = Span::new(ByteOffset::new(5), ByteOffset::new(6)).unwrap();
        assert_eq!(
            a.intersect(b),
            Span::new(ByteOffset::new(2), ByteOffset::new(4)).ok()
        );
        // Touching spans intersect in the empty span at the touch point:
        // interval semantics, which is what keeps intersect consistent
        // with contains_span for empty operands (base.md §10).
        assert_eq!(a.intersect(c), Some(Span::empty(ByteOffset::new(4))));
        assert_eq!(a.intersect(d), None);
    }

    #[test]
    fn ordering_is_document_order_with_shorter_first_ties() {
        let early = Span::new(ByteOffset::new(1), ByteOffset::new(9)).unwrap();
        let late = Span::new(ByteOffset::new(2), ByteOffset::new(3)).unwrap();
        let late_longer = Span::new(ByteOffset::new(2), ByteOffset::new(4)).unwrap();
        assert!(early < late);
        assert!(late < late_longer);
    }

    #[test]
    fn checked_arithmetic_refuses_overflow() {
        assert_eq!(ByteOffset::new(u32::MAX).checked_add(1), None);
        assert_eq!(ByteOffset::ZERO.checked_sub(1), None);
        assert_eq!(ByteOffset::new(3).checked_add(4), Some(ByteOffset::new(7)));
    }

    #[test]
    fn covering_orders_its_offsets() {
        let ordered = Span::covering(ByteOffset::new(2), ByteOffset::new(5));
        let reversed = Span::covering(ByteOffset::new(5), ByteOffset::new(2));
        assert_eq!(
            ordered,
            Span::new(ByteOffset::new(2), ByteOffset::new(5)).unwrap()
        );
        assert_eq!(reversed, ordered);
        assert_eq!(
            Span::covering(ByteOffset::new(4), ByteOffset::new(4)),
            Span::empty(ByteOffset::new(4))
        );
    }

    #[test]
    fn location_orders_by_source_then_span() {
        let a = Location {
            source: SourceId::new(1),
            span: Span::new(ByteOffset::new(9), ByteOffset::new(10)).unwrap(),
        };
        let b = Location {
            source: SourceId::new(2),
            span: Span::new(ByteOffset::new(0), ByteOffset::new(1)).unwrap(),
        };
        assert!(a < b);
    }

    #[test]
    fn end_before_start_displays_the_fixable_question() {
        let refusal = EndBeforeStart {
            start: ByteOffset::new(5),
            end: ByteOffset::new(3),
        };
        assert_eq!(refusal.to_string(), "span end 3 is before its start 5");
        let _: &dyn std::error::Error = &refusal;
    }
}
