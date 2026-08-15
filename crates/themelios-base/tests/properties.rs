//! The stage-1 property laws (docs/design/base.md §10), held by
//! proptest over the public surface only.

use proptest::prelude::*;
use themelios_base::span::{ByteOffset, Span};

fn spans() -> impl Strategy<Value = Span> {
    (any::<u32>(), any::<u32>()).prop_map(|(a, b)| {
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        Span::new(ByteOffset::new(start), ByteOffset::new(end)).expect("endpoints were ordered")
    })
}

proptest! {
    #[test]
    fn join_is_idempotent(a in spans()) {
        prop_assert_eq!(a.join(a), a);
    }

    #[test]
    fn join_is_commutative(a in spans(), b in spans()) {
        prop_assert_eq!(a.join(b), b.join(a));
    }

    #[test]
    fn join_is_associative(a in spans(), b in spans(), c in spans()) {
        prop_assert_eq!(a.join(b).join(c), a.join(b.join(c)));
    }

    #[test]
    fn intersect_is_consistent_with_contains_span(
        a in spans(),
        b in spans(),
    ) {
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

mod source_admission {
    use proptest::prelude::*;
    use themelios_base::source::{FromBytesRefusal, Source, SourceId};

    proptest! {
        /// base.md §10: `from_bytes` on arbitrary bytes never panics
        /// and refuses exactly when the standard library's validator
        /// does. (`TooLarge` is unreachable at generated sizes.)
        #[test]
        fn from_bytes_agrees_with_the_std_validator(
            bytes in proptest::collection::vec(any::<u8>(), 0..2048),
        ) {
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
    use themelios_base::line::{ColumnEncoding, LineCol, LineIndex, PositionRefusal};
    use themelios_base::source::{NotCharBoundary, Source, SourceId};
    use themelios_base::span::ByteOffset;

    const ENCODINGS: [ColumnEncoding; 3] = [
        ColumnEncoding::Utf8Bytes,
        ColumnEncoding::CodePoints,
        ColumnEncoding::Utf16Units,
    ];

    /// Multi-byte-heavy generated text (base.md §10): ASCII, two-,
    /// three-, and four-byte characters, plus every newline shape.
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
                col += match encoding {
                    ColumnEncoding::Utf8Bytes => character.len_utf8() as u32,
                    ColumnEncoding::CodePoints => 1,
                    ColumnEncoding::Utf16Units => character.len_utf16() as u32,
                };
            }
        }
        LineCol { line, col }
    }

    proptest! {
        /// base.md §10: oracle agreement on every boundary, the
        /// round-trip identity in every encoding, refusal on every
        /// non-boundary, refusal past the end.
        #[test]
        fn conversions_agree_with_the_oracle_and_round_trip(
            text in multibyte_text(),
        ) {
            let source =
                Source::new(SourceId::new(0), text.clone())
                    .expect("generated text admits");
            let index = LineIndex::of(&source);
            let boundaries: Vec<usize> = text
                .char_indices()
                .map(|(i, _)| i)
                .chain([text.len()])
                .collect();
            for &encoding in &ENCODINGS {
                for &boundary in &boundaries {
                    let offset = ByteOffset::new(boundary as u32);
                    let position = index
                        .position(offset, encoding)
                        .expect("boundary offsets position");
                    prop_assert_eq!(
                        position,
                        oracle(&text, boundary, encoding)
                    );
                    prop_assert_eq!(
                        index.offset(position, encoding),
                        Ok(offset)
                    );
                }
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
                prop_assert!(matches!(
                    index.position(
                        ByteOffset::new(text.len() as u32 + 1),
                        encoding,
                    ),
                    Err(PositionRefusal::OutOfBounds(_))
                ));
            }
        }
    }
}
