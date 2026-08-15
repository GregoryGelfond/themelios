//! The source-text model (docs/design/base.md §3): text with an
//! identity, from anywhere — this crate does no I/O and never sees a
//! path. Admission is the one well-formedness authority for text;
//! everything downstream rides on a `Source` and inherits its
//! guarantees.

use std::fmt;

use crate::line::LineIndex;
use crate::span::{ByteOffset, Span};

/// An opaque identity for one source text. The embedding host mints it,
/// because the host already has it (base.md §3.1).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SourceId(u32);

impl SourceId {
    /// Wraps a host-minted identity. Total; O(1).
    pub const fn new(raw: u32) -> SourceId {
        SourceId(raw)
    }

    /// The raw identity. Total; O(1).
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Text longer than `Source::MAX_LEN` bytes (base.md §3.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TooLarge {
    /// The offered length, in bytes.
    pub len: usize,
}

/// Bytes that are not valid UTF-8; `valid_up_to` mirrors the standard
/// library's error detail (base.md §3.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InvalidUtf8 {
    /// How many leading bytes were valid.
    pub valid_up_to: usize,
}

/// What `Source::from_bytes` can refuse (base.md §3.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FromBytesRefusal {
    /// The byte count exceeds the admission ceiling.
    TooLarge(TooLarge),
    /// The bytes are not valid UTF-8.
    InvalidUtf8(InvalidUtf8),
}

/// The shared boundary condition (base.md §3.2): an offset inside a
/// multi-byte character. Defined once, at the first door where span
/// meets text; the line module's position queries wrap the same
/// condition.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct NotCharBoundary {
    /// The offending offset.
    pub offset: ByteOffset,
}

/// What `Source::slice` can refuse (base.md §3.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SliceRefusal {
    /// The span ends past the text.
    OutOfBounds {
        /// The span's one-past-end offset.
        end: ByteOffset,
        /// The text's one-past-end offset.
        max: ByteOffset,
    },
    /// A span endpoint falls inside a multi-byte character.
    NotCharBoundary(NotCharBoundary),
}

impl fmt::Display for TooLarge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "text is {} bytes; the admission ceiling Source::MAX_LEN \
             is {} bytes",
            self.len,
            Source::MAX_LEN
        )
    }
}

impl std::error::Error for TooLarge {}

impl fmt::Display for InvalidUtf8 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "bytes are not valid UTF-8 from byte {}",
            self.valid_up_to
        )
    }
}

impl std::error::Error for InvalidUtf8 {}

impl fmt::Display for FromBytesRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FromBytesRefusal::TooLarge(refusal) => refusal.fmt(f),
            FromBytesRefusal::InvalidUtf8(refusal) => refusal.fmt(f),
        }
    }
}

// The wrapper is the condition itself (base.md §3.2), not a layer over a
// lower-level cause: it forwards Display and reports no `source()`, so a
// host's error chain states each refusal once.
impl std::error::Error for FromBytesRefusal {}

impl fmt::Display for NotCharBoundary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "byte {} is not a character boundary", self.offset.get())
    }
}

impl std::error::Error for NotCharBoundary {}

impl fmt::Display for SliceRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SliceRefusal::OutOfBounds { end, max } => write!(
                f,
                "span ends at byte {}, past the text's end at byte {}",
                end.get(),
                max.get()
            ),
            SliceRefusal::NotCharBoundary(refusal) => refusal.fmt(f),
        }
    }
}

impl std::error::Error for SliceRefusal {}

/// One owned source text and its identity. UTF-8 by construction
/// (base.md §3.2): arbitrary bytes meet a typed refusal at admission,
/// and everything past the door is valid UTF-8.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Source {
    id: SourceId,
    text: String,
}

impl Source {
    /// The admission ceiling: offsets are `u32` and every text has
    /// one more line start than newline bytes, so text is at most
    /// `u32::MAX - 1` bytes — the line count then fits `u32` for
    /// every admissible text. The name exists so the limit is never
    /// a bare numeral at a call site (base.md §3.2).
    pub const MAX_LEN: usize = u32::MAX as usize - 1;

    /// The one admission-ceiling check, shared by both doors: a length
    /// admits exactly when it is at most `MAX_LEN`. O(1).
    fn admit_len(len: usize) -> Result<(), TooLarge> {
        if len > Source::MAX_LEN {
            Err(TooLarge { len })
        } else {
            Ok(())
        }
    }

    /// Admits owned text. Refuses `TooLarge`; O(1) beyond the length
    /// check. No repair at the door: no BOM stripping, no line-ending
    /// normalization — author bytes are data (base.md §3.2).
    pub fn new(id: SourceId, text: String) -> Result<Source, TooLarge> {
        Source::admit_len(text.len())?;
        Ok(Source { id, text })
    }

    /// Admits raw bytes. Refuses `FromBytesRefusal` — the length check
    /// first, then UTF-8 validation; O(n) validation (base.md §3.2).
    pub fn from_bytes(id: SourceId, bytes: Vec<u8>) -> Result<Source, FromBytesRefusal> {
        Source::admit_len(bytes.len()).map_err(FromBytesRefusal::TooLarge)?;
        match String::from_utf8(bytes) {
            Ok(text) => Ok(Source { id, text }),
            Err(error) => Err(FromBytesRefusal::InvalidUtf8(InvalidUtf8 {
                valid_up_to: error.utf8_error().valid_up_to(),
            })),
        }
    }

    /// The identity the host minted. Total; O(1).
    pub fn id(&self) -> SourceId {
        self.id
    }

    /// The admitted text. Total; O(1).
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The covering span: `ByteOffset::ZERO` to the one-past-end
    /// offset. Total; O(1).
    pub fn span(&self) -> Span {
        Span::covering(ByteOffset::ZERO, self.end())
    }

    /// The one-past-end offset. Total; O(1).
    pub fn end(&self) -> ByteOffset {
        // The cast cannot truncate: admission guards
        // len <= MAX_LEN < u32::MAX.
        ByteOffset::new(self.text.len() as u32)
    }

    /// The spanned text. Refuses out-of-bounds and non-boundary
    /// endpoints (`SliceRefusal`) — the end against the text's extent
    /// first, then the start and the end as character boundaries, in
    /// that order; O(1) against the owned text (base.md §3.2).
    pub fn slice(&self, span: Span) -> Result<&str, SliceRefusal> {
        let max = self.end();
        if span.end() > max {
            return Err(SliceRefusal::OutOfBounds {
                end: span.end(),
                max,
            });
        }
        let start = span.start().get() as usize;
        let end = span.end().get() as usize;
        if !self.text.is_char_boundary(start) {
            return Err(SliceRefusal::NotCharBoundary(NotCharBoundary {
                offset: span.start(),
            }));
        }
        if !self.text.is_char_boundary(end) {
            return Err(SliceRefusal::NotCharBoundary(NotCharBoundary {
                offset: span.end(),
            }));
        }
        Ok(&self.text[start..end])
    }
}

/// The view environment: resolves identity to display data. Two laws
/// bind every implementor (base.md §3.4):
///
/// 1. **Completeness.** For a given id, answer all three accessors or
///    none. Partial resolution is a contract breach, not a state the
///    views must interpret.
/// 2. **Coherence.** `line_index(id)` is observationally
///    `LineIndex::of` applied to an admitted `Source` of `text(id)` —
///    the index *of* that text, not of any earlier version of it.
///
/// Unknown identities answer `None` — a refusal, never a panic. The
/// name is whatever the host declares: display data, not a path. What
/// the views can check, they check — a completeness breach is named
/// at view time. Coherence is not cheaply checkable there, so the
/// views trust it; that trust is this contract's stated boundary, and
/// `check_sources_laws` is the test-time instrument that holds it.
pub trait Sources {
    /// The display name the host declares for this source.
    fn name(&self, id: SourceId) -> Option<&str>;
    /// The source's text.
    fn text(&self, id: SourceId) -> Option<&str>;
    /// The line index of exactly that text.
    fn line_index(&self, id: SourceId) -> Option<&LineIndex>;
}

/// Which accessor a resolution lacked — used by the law checker's
/// report, the editor view's refusal, and the human view's
/// placeholder (base.md §3.4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SourceFacet {
    /// `Sources::name` answered `None`.
    Name,
    /// `Sources::text` answered `None`.
    Text,
    /// `Sources::line_index` answered `None`.
    Index,
}

/// One law breach found by `check_sources_laws` (base.md §3.4).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SourcesLawViolation {
    /// The id resolved some facets but not all three.
    Incomplete {
        /// The id whose resolution was partial.
        id: SourceId,
        /// A facet that did not resolve.
        missing: SourceFacet,
    },
    /// The resolved index is not the index of the resolved text.
    IncoherentIndex {
        /// The id whose index disagrees with its text.
        id: SourceId,
    },
}

/// The laws, checkable: verifies completeness per id and coherence by
/// re-deriving the index from the resolved text. Deterministic and
/// total — an empty report is the laws holding over those ids;
/// O(total resolved text). Implementors run it in their own tests
/// (base.md §3.4).
///
/// Ids are checked in the order given, one `Incomplete` per missing
/// facet in `Name`, `Text`, `Index` order; coherence is checked
/// whenever text and index both resolve, independent of the
/// completeness verdict.
pub fn check_sources_laws(sources: &impl Sources, ids: &[SourceId]) -> Vec<SourcesLawViolation> {
    let mut violations = Vec::new();
    for &id in ids {
        let name = sources.name(id);
        let text = sources.text(id);
        let index = sources.line_index(id);
        let facets = [
            (name.is_some(), SourceFacet::Name),
            (text.is_some(), SourceFacet::Text),
            (index.is_some(), SourceFacet::Index),
        ];
        let resolved = facets.iter().filter(|(present, _)| *present).count();
        if 0 < resolved && resolved < facets.len() {
            for (present, facet) in facets {
                if !present {
                    violations.push(SourcesLawViolation::Incomplete { id, missing: facet });
                }
            }
        }
        if let (Some(text), Some(index)) = (text, index) {
            // A text the admission door refuses cannot have an index
            // that is LineIndex::of over an admitted Source of it —
            // incoherent by definition.
            let coherent = Source::new(id, text.to_owned())
                .is_ok_and(|admitted| LineIndex::of(&admitted) == *index);
            if !coherent {
                violations.push(SourcesLawViolation::IncoherentIndex { id });
            }
        }
    }
    violations
}

/// A Vec-backed catalog that mints ids sequentially — a host you can
/// use, not this crate seizing minting. Satisfies both laws by
/// construction: it admits under the `Source` doors and builds each
/// `LineIndex` eagerly at `add` — an explicit derivation, not lazy
/// state. Deliberately not a virtual file system, and it never grows
/// toward one: no paths, no watching, no loading (base.md §3.4, §11).
#[derive(Clone, Debug)]
pub struct SourceSet {
    entries: Vec<SourceSetEntry>,
}

#[derive(Clone, Debug)]
struct SourceSetEntry {
    name: String,
    source: Source,
    index: LineIndex,
}

impl SourceSet {
    /// An empty catalog. Total; O(1).
    pub fn new() -> SourceSet {
        SourceSet {
            entries: Vec::new(),
        }
    }

    /// Admits one source under the `Source` doors, builds its index
    /// eagerly, and mints the next sequential id. Refuses `TooLarge`;
    /// O(n) — admission plus the index build (base.md §3.4, §9).
    pub fn add(&mut self, name: String, text: String) -> Result<SourceId, TooLarge> {
        // The cast cannot truncate in any real embedding: entries own
        // their text, so a wrapped mint would need more entries than
        // memory holds bytes.
        let id = SourceId::new(self.entries.len() as u32);
        let source = Source::new(id, text)?;
        let index = LineIndex::of(&source);
        self.entries.push(SourceSetEntry {
            name,
            source,
            index,
        });
        Ok(id)
    }

    fn entry(&self, id: SourceId) -> Option<&SourceSetEntry> {
        self.entries.get(id.get() as usize)
    }
}

impl Default for SourceSet {
    // The std idiom for an argument-free constructor;
    // clippy::new_without_default holds it.
    fn default() -> SourceSet {
        SourceSet::new()
    }
}

impl Sources for SourceSet {
    fn name(&self, id: SourceId) -> Option<&str> {
        self.entry(id).map(|entry| entry.name.as_str())
    }

    fn text(&self, id: SourceId) -> Option<&str> {
        self.entry(id).map(|entry| entry.source.text())
    }

    fn line_index(&self, id: SourceId) -> Option<&LineIndex> {
        self.entry(id).map(|entry| &entry.index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::{ByteOffset, Span};

    fn id() -> SourceId {
        SourceId::new(7)
    }

    #[test]
    fn admission_keeps_text_and_identity() {
        let source = Source::new(id(), "p(a).\n".to_owned()).expect("small text admits");
        assert_eq!(source.id(), id());
        assert_eq!(source.text(), "p(a).\n");
        assert_eq!(source.end(), ByteOffset::new(6));
        assert_eq!(source.span().start(), ByteOffset::ZERO);
        assert_eq!(source.span().end(), ByteOffset::new(6));
    }

    #[test]
    fn admission_ceiling_is_the_boundary() {
        // The check both doors share, held at its boundary without a
        // four-gibibyte witness.
        assert_eq!(Source::admit_len(Source::MAX_LEN), Ok(()));
        assert_eq!(
            Source::admit_len(Source::MAX_LEN + 1),
            Err(TooLarge {
                len: Source::MAX_LEN + 1
            })
        );
    }

    #[test]
    fn admission_ceiling_fits_the_line_count() {
        // A text of `len` bytes has at most `len + 1` lines, so the line
        // count of every admissible text must fit the index's u32.
        assert!(u32::try_from(Source::MAX_LEN + 1).is_ok());
    }

    #[test]
    fn from_bytes_admits_valid_utf8_and_refuses_invalid() {
        let ok = Source::from_bytes(id(), b"q(b).".to_vec());
        assert_eq!(ok.expect("valid UTF-8 admits").text(), "q(b).");

        let refused = Source::from_bytes(id(), vec![0x70, 0xFF, 0x70]);
        assert_eq!(
            refused,
            Err(FromBytesRefusal::InvalidUtf8(InvalidUtf8 {
                valid_up_to: 1
            }))
        );
    }

    #[test]
    fn no_repair_at_the_door() {
        // BOM, CRLF, and a lone CR pass through byte-for-byte
        // (base.md §3.2: author bytes are data).
        let text = "\u{FEFF}a\r\nb\rc";
        let source = Source::new(id(), text.to_owned()).unwrap();
        assert_eq!(source.text(), text);
    }

    #[test]
    fn slice_returns_the_spanned_text() {
        let source = Source::new(id(), "héllo".to_owned()).unwrap();
        let span = Span::new(ByteOffset::new(1), ByteOffset::new(3)).unwrap();
        assert_eq!(source.slice(span), Ok("é"));
    }

    #[test]
    fn slice_admits_a_span_ending_at_the_text_end() {
        // The end-of-text position is in bounds: the covering span
        // slices to the whole text.
        let source = Source::new(id(), "héllo".to_owned()).unwrap();
        assert_eq!(source.slice(source.span()), Ok("héllo"));
    }

    #[test]
    fn slice_refuses_out_of_bounds_with_both_facts() {
        let source = Source::new(id(), "abc".to_owned()).unwrap();
        let span = Span::new(ByteOffset::new(1), ByteOffset::new(9)).unwrap();
        assert_eq!(
            source.slice(span),
            Err(SliceRefusal::OutOfBounds {
                end: ByteOffset::new(9),
                max: ByteOffset::new(3),
            })
        );
    }

    #[test]
    fn slice_refuses_a_mid_character_boundary() {
        let source = Source::new(id(), "héllo".to_owned()).unwrap();
        // Byte 2 is inside the two-byte 'é'.
        let span = Span::new(ByteOffset::new(2), ByteOffset::new(3)).unwrap();
        assert_eq!(
            source.slice(span),
            Err(SliceRefusal::NotCharBoundary(NotCharBoundary {
                offset: ByteOffset::new(2)
            }))
        );
    }

    #[test]
    fn refusals_display_the_fixable_question() {
        assert_eq!(
            TooLarge {
                len: Source::MAX_LEN + 1
            }
            .to_string(),
            "text is 4294967295 bytes; the admission ceiling \
             Source::MAX_LEN is 4294967294 bytes"
        );
        assert_eq!(
            InvalidUtf8 { valid_up_to: 12 }.to_string(),
            "bytes are not valid UTF-8 from byte 12"
        );
        let _: &dyn std::error::Error = &TooLarge { len: 0 };
        let _: &dyn std::error::Error = &SliceRefusal::NotCharBoundary(NotCharBoundary {
            offset: ByteOffset::ZERO,
        });
    }
}
