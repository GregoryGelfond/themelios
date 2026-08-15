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
