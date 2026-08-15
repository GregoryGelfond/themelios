//! The stage-1 property laws (docs/design/base.md §10), held by
//! proptest over the public surface only.

use proptest::prelude::*;

/// Multi-byte-heavy generated text (base.md §10): ASCII, two-, three-,
/// and four-byte characters, plus every newline shape.
fn multibyte_text() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            Just("a"),
            Just("Z"),
            Just(" "),
            Just("é"),
            Just("√"),
            Just("你"),
            Just("🦀"),
            Just("\n"),
            Just("\r"),
            Just("\r\n"),
        ],
        0..120,
    )
    .prop_map(|pieces| pieces.concat())
}

mod span_algebra {
    use proptest::prelude::*;
    use themelios_base::span::{ByteOffset, Span};

    /// Spans with ordered endpoints; one draw in four is empty, so the
    /// laws meet the empty operand the touch-point semantics exist for.
    fn spans() -> impl Strategy<Value = Span> {
        prop_oneof![
            3 => (any::<u32>(), any::<u32>()).prop_map(|(a, b)| {
                let (start, end) = if a <= b { (a, b) } else { (b, a) };
                Span::new(ByteOffset::new(start), ByteOffset::new(end))
                    .expect("endpoints were ordered")
            }),
            1 => any::<u32>().prop_map(|at| Span::empty(ByteOffset::new(at))),
        ]
    }

    /// Pairs of spans: independent, touching at the first's end, or the
    /// second nested in the first — so intersection meets every shape.
    fn span_pairs() -> impl Strategy<Value = (Span, Span)> {
        prop_oneof![
            2 => (spans(), spans()),
            1 => (spans(), any::<u32>()).prop_map(|(a, extent)| {
                let end = ByteOffset::new(a.end().get().saturating_add(extent));
                (a, Span::new(a.end(), end).expect("touching span is ordered"))
            }),
            1 => (spans(), any::<u32>(), any::<u32>()).prop_map(|(a, x, y)| {
                // Positions within `a` are 0..=len; the modulus is taken in
                // u64 so a full-range span cannot overflow it, and the cast
                // cannot truncate: the remainder is at most len.
                let positions = u64::from(a.len()) + 1;
                let inner = |raw: u32| {
                    ByteOffset::new(a.start().get() + (u64::from(raw) % positions) as u32)
                };
                let (s, e) = (inner(x), inner(y));
                let (s, e) = if s <= e { (s, e) } else { (e, s) };
                (a, Span::new(s, e).expect("nested span is ordered"))
            }),
        ]
    }

    proptest! {
        #[test]
        fn join_is_idempotent(a in spans()) {
            prop_assert_eq!(a.join(a), a);
        }

        #[test]
        fn join_is_commutative((a, b) in span_pairs()) {
            prop_assert_eq!(a.join(b), b.join(a));
        }

        #[test]
        fn join_is_associative(a in spans(), b in spans(), c in spans()) {
            prop_assert_eq!(a.join(b).join(c), a.join(b.join(c)));
        }

        #[test]
        fn intersect_is_consistent_with_contains_span((a, b) in span_pairs()) {
            // Containment means intersection is the contained span; any
            // intersection lies within both operands.
            if a.contains_span(b) {
                prop_assert_eq!(a.intersect(b), Some(b));
            }
            if let Some(both) = a.intersect(b) {
                prop_assert!(a.contains_span(both));
                prop_assert!(b.contains_span(both));
            }
            prop_assert_eq!(a.intersect(b), b.intersect(a));
        }
    }
}

mod source_admission {
    use proptest::prelude::*;
    use themelios_base::source::{FromBytesRefusal, Source, SourceId};

    /// Byte vectors that reach every outcome: arbitrary bytes (almost
    /// always refused), valid multi-byte text (always admitted), and
    /// valid text with one byte overwritten (refused at a position the
    /// validator names).
    fn bytes() -> impl Strategy<Value = Vec<u8>> {
        prop_oneof![
            proptest::collection::vec(any::<u8>(), 0..2048),
            super::multibyte_text().prop_map(String::into_bytes),
            (super::multibyte_text(), any::<usize>(), any::<u8>()).prop_map(|(text, at, byte)| {
                let mut bytes = text.into_bytes();
                if !bytes.is_empty() {
                    let at = at % bytes.len();
                    bytes[at] = byte;
                }
                bytes
            }),
        ]
    }

    proptest! {
        /// base.md §10: `from_bytes` on arbitrary bytes never panics
        /// and refuses exactly when the standard library's validator
        /// does. (`TooLarge` is unreachable at generated sizes.)
        #[test]
        fn from_bytes_agrees_with_the_std_validator(bytes in bytes()) {
            let admitted =
                Source::from_bytes(SourceId::new(0), bytes.clone());
            match std::str::from_utf8(&bytes) {
                Ok(_) => prop_assert!(admitted.is_ok()),
                Err(error) => match admitted {
                    Err(FromBytesRefusal::InvalidUtf8(refusal)) => {
                        prop_assert_eq!(
                            refusal.valid_up_to,
                            error.valid_up_to()
                        );
                    }
                    other => prop_assert!(
                        false,
                        "expected InvalidUtf8, got {:?}",
                        other
                    ),
                },
            }
        }
    }
}

mod line_conversions {
    use proptest::prelude::*;
    use themelios_base::line::{
        ColumnEncoding, ColumnNotBoundary, ColumnOutOfBounds, LineCol, LineIndex, LineOutOfBounds,
        OffsetOutOfBounds, OffsetRefusal, PositionRefusal,
    };
    use themelios_base::source::{NotCharBoundary, Source, SourceId};
    use themelios_base::span::ByteOffset;

    const ENCODINGS: [ColumnEncoding; 3] = [
        ColumnEncoding::Utf8Bytes,
        ColumnEncoding::CodePoints,
        ColumnEncoding::Utf16Units,
    ];

    /// The width of one character in the encoding's units.
    fn unit_len(character: char, encoding: ColumnEncoding) -> u32 {
        match encoding {
            ColumnEncoding::Utf8Bytes => character.len_utf8() as u32,
            ColumnEncoding::CodePoints => 1,
            ColumnEncoding::Utf16Units => character.len_utf16() as u32,
        }
    }

    /// The naive character-walk oracle: recompute the coordinate from
    /// scratch, one character at a time.
    fn oracle(text: &str, target: usize, encoding: ColumnEncoding) -> LineCol {
        let (mut line, mut col) = (0u32, 0u32);
        for (i, character) in text.char_indices() {
            if i >= target {
                break;
            }
            if character == '\n' {
                line += 1;
                col = 0;
            } else {
                col += unit_len(character, encoding);
            }
        }
        LineCol { line, col }
    }

    /// The lines of a text as the index sees them: content without its
    /// terminator, split at `\n` alone.
    fn lines(text: &str) -> Vec<&str> {
        text.split('\n').collect()
    }

    /// The columns of one line that are unit boundaries in the encoding,
    /// by walking its characters — 0, then each cumulative width.
    fn boundary_columns(line: &str, encoding: ColumnEncoding) -> Vec<u32> {
        let mut columns = vec![0u32];
        let mut col = 0u32;
        for character in line.chars() {
            col += unit_len(character, encoding);
            columns.push(col);
        }
        columns
    }

    fn index_of(text: &str) -> LineIndex {
        let source = Source::new(SourceId::new(0), text.to_owned()).expect("generated text admits");
        LineIndex::of(&source)
    }

    /// Every character boundary of the text, and the end-of-text position.
    fn boundaries(text: &str) -> Vec<usize> {
        text.char_indices()
            .map(|(i, _)| i)
            .chain([text.len()])
            .collect()
    }

    proptest! {
        /// base.md §10: `LineIndex` agreement with a naive
        /// character-walk oracle, on every boundary, in every encoding.
        #[test]
        fn position_agrees_with_the_character_walk_oracle(
            text in super::multibyte_text(),
        ) {
            let index = index_of(&text);
            for &encoding in &ENCODINGS {
                for &boundary in &boundaries(&text) {
                    let offset = ByteOffset::new(boundary as u32);
                    prop_assert_eq!(
                        index.position(offset, encoding),
                        Ok(oracle(&text, boundary, encoding))
                    );
                }
            }
        }

        /// base.md §10: `offset → position → offset` identity in all
        /// three encodings, for every valid offset.
        #[test]
        fn offset_inverts_position_in_every_encoding(
            text in super::multibyte_text(),
        ) {
            let index = index_of(&text);
            for &encoding in &ENCODINGS {
                for &boundary in &boundaries(&text) {
                    let offset = ByteOffset::new(boundary as u32);
                    let position = index
                        .position(offset, encoding)
                        .expect("boundary offsets position");
                    prop_assert_eq!(index.offset(position, encoding), Ok(offset));
                }
            }
        }

        /// base.md §5: a mid-character offset is refused, never
        /// misplaced — for every non-boundary byte, in every encoding.
        #[test]
        fn position_refuses_every_non_boundary(text in super::multibyte_text()) {
            let index = index_of(&text);
            for &encoding in &ENCODINGS {
                for byte in 0..text.len() {
                    if !text.is_char_boundary(byte) {
                        let offset = ByteOffset::new(byte as u32);
                        prop_assert_eq!(
                            index.position(offset, encoding),
                            Err(PositionRefusal::NotCharBoundary(
                                NotCharBoundary { offset }
                            ))
                        );
                    }
                }
            }
        }

        /// base.md §5: offsets `0..=len` are in bounds; `len + 1` is
        /// refused as out of bounds, naming the end-of-text position.
        #[test]
        fn position_refuses_past_the_end(text in super::multibyte_text()) {
            let index = index_of(&text);
            let max = ByteOffset::new(text.len() as u32);
            let past = ByteOffset::new(text.len() as u32 + 1);
            for &encoding in &ENCODINGS {
                prop_assert_eq!(
                    index.position(past, encoding),
                    Err(PositionRefusal::OutOfBounds(OffsetOutOfBounds { offset: past, max }))
                );
            }
        }

        /// base.md §5: a line at the line count is refused, naming the
        /// count.
        #[test]
        fn offset_refuses_lines_past_the_count(text in super::multibyte_text()) {
            let index = index_of(&text);
            let line_count = lines(&text).len() as u32;
            prop_assert_eq!(index.line_count(), line_count);
            for &encoding in &ENCODINGS {
                prop_assert_eq!(
                    index.offset(LineCol { line: line_count, col: 0 }, encoding),
                    Err(OffsetRefusal::LineOutOfBounds(LineOutOfBounds {
                        line: line_count,
                        line_count,
                    }))
                );
            }
        }

        /// base.md §5: a column past a line's extent is refused, and
        /// `max` is that extent in the encoding the query stated.
        #[test]
        fn offset_refuses_columns_past_the_extent(text in super::multibyte_text()) {
            let index = index_of(&text);
            for &encoding in &ENCODINGS {
                for (line, content) in lines(&text).iter().enumerate() {
                    let line = line as u32;
                    let max = *boundary_columns(content, encoding)
                        .last()
                        .expect("a line has a zero column");
                    let col = max + 1;
                    prop_assert_eq!(
                        index.offset(LineCol { line, col }, encoding),
                        Err(OffsetRefusal::ColumnOutOfBounds(ColumnOutOfBounds {
                            line,
                            col,
                            max,
                        }))
                    );
                }
            }
        }

        /// base.md §5: a column inside a multi-unit character — a byte
        /// of a multi-byte character, the second unit of a UTF-16
        /// surrogate pair — is refused, never misplaced.
        #[test]
        fn offset_refuses_columns_inside_a_character(text in super::multibyte_text()) {
            let index = index_of(&text);
            for &encoding in &ENCODINGS {
                for (line, content) in lines(&text).iter().enumerate() {
                    let line = line as u32;
                    let columns = boundary_columns(content, encoding);
                    let extent = *columns.last().expect("a line has a zero column");
                    for col in 0..=extent {
                        if columns.binary_search(&col).is_err() {
                            prop_assert_eq!(
                                index.offset(LineCol { line, col }, encoding),
                                Err(OffsetRefusal::ColumnNotBoundary(ColumnNotBoundary {
                                    line,
                                    col,
                                }))
                            );
                        }
                    }
                }
            }
        }
    }
}

mod human_totality {
    use proptest::prelude::*;
    use themelios_base::diagnostic::{Diagnostic, DiagnosticId, Label, Severity};
    use themelios_base::line::LineIndex;
    use themelios_base::source::{SourceId, SourceSet, Sources};
    use themelios_base::span::{ByteOffset, Location, Span};
    use themelios_base::view::human;

    const IDS: [DiagnosticId; 2] = [
        DiagnosticId::new("syntax", "unexpected-token"),
        DiagnosticId::new("program", "unknown-name"),
    ];

    /// A catalog wrapper that drops facets per a generated mask —
    /// the incomplete resolutions base.md §10 names as included in
    /// the law's catalog space.
    struct FacetDropping {
        inner: SourceSet,
        /// Kept-bits (name, text, index) per id; ids past the mask
        /// resolve as the inner catalog answers.
        mask: Vec<(bool, bool, bool)>,
    }

    impl FacetDropping {
        fn kept(&self, id: SourceId) -> (bool, bool, bool) {
            self.mask
                .get(id.get() as usize)
                .copied()
                .unwrap_or((true, true, true))
        }
    }

    impl Sources for FacetDropping {
        fn name(&self, id: SourceId) -> Option<&str> {
            self.kept(id).0.then(|| self.inner.name(id)).flatten()
        }

        fn text(&self, id: SourceId) -> Option<&str> {
            self.kept(id).1.then(|| self.inner.text(id)).flatten()
        }

        fn line_index(&self, id: SourceId) -> Option<&LineIndex> {
            self.kept(id).2.then(|| self.inner.line_index(id)).flatten()
        }
    }

    fn labels() -> impl Strategy<Value = Label> {
        // Sources 0..6 over a three-entry catalog, spans up to byte
        // 40 over shorter texts: unresolvable ids, out-of-bounds and
        // mid-character spans included by construction (base.md §10).
        (
            0u32..6,
            0u32..40,
            0u32..8,
            proptest::option::of("[a-z]{0,6}"),
        )
            .prop_map(|(source, start, extra, message)| Label {
                location: Location {
                    source: SourceId::new(source),
                    span: Span::new(ByteOffset::new(start), ByteOffset::new(start + extra))
                        .expect("ordered endpoints"),
                },
                message,
            })
    }

    proptest! {
        /// base.md §10: `view::human` total on arbitrary well-formed
        /// diagnostics over arbitrary catalogs — unresolvable ids,
        /// incomplete resolutions, and span/text mismatches included.
        #[test]
        fn human_is_total(
            id_choice in 0usize..2,
            severity_choice in 0usize..3,
            message in "[a-z ]{1,20}",
            primary in labels(),
            secondaries in proptest::collection::vec(labels(), 0..4),
            notes in proptest::collection::vec("[a-z ]{0,12}", 0..3),
            helps in proptest::collection::vec("[a-z ]{0,12}", 0..3),
            mask in proptest::collection::vec(
                (any::<bool>(), any::<bool>(), any::<bool>()),
                3,
            ),
        ) {
            let mut inner = SourceSet::new();
            for text in ["p(a).\nq(b).\n", "héllo\n🦀 line\n", ""] {
                inner
                    .add("gen.lp".to_owned(), text.to_owned())
                    .expect("small text admits");
            }
            let catalog = FacetDropping { inner, mask };
            let severity = [
                Severity::Note,
                Severity::Warning,
                Severity::Error,
            ][severity_choice];
            let mut diagnostic = Diagnostic::new(
                IDS[id_choice],
                severity,
                message,
                primary,
            )
            .expect("generated headline is non-empty");
            for secondary in secondaries {
                diagnostic = diagnostic.with_secondary(secondary);
            }
            for note in notes {
                diagnostic = diagnostic.with_note(note);
            }
            for help in helps {
                diagnostic = diagnostic.with_help(help);
            }
            let rendered = human(&diagnostic, &catalog);
            // Total and never silent: the headline always leads. (Bound
            // first: `prop_assert!` stringifies its condition into the
            // failure message, where a format string's braces would read
            // as placeholders.)
            let headline = format!("{}[{}]: ", severity, IDS[id_choice]);
            prop_assert!(rendered.starts_with(&headline));
            prop_assert!(rendered.ends_with('\n'));
        }
    }
}
