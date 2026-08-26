//! Laws of in-node provenance (docs/design/program.md §6): the carrier's identity
//! erases provenance (equal content compares, orders, and hashes equal regardless of
//! provenance; a set dedupes by content), the merge is a bounded join-semilattice,
//! and `map` carries provenance through a content rewrite.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use proptest::prelude::*;
use themelios_program::provenance::{Origin, Provenance, TransformTag, WithProvenance};

#[test]
fn provenance_is_erased_from_the_carrier_s_identity() {
    let one = Provenance::from(Origin::Constructed);
    let another = Provenance::from(Origin::Transformed(TransformTag::new("t"))).with_doc("d");
    // Same content, different provenance: equal, order-equal, hash-equal.
    let x = WithProvenance::new(7_i32, one.clone());
    let y = WithProvenance::new(7_i32, another);
    assert_eq!(x, y);
    assert_eq!(x.cmp(&y), Ordering::Equal);
    let (mut hx, mut hy) = (DefaultHasher::new(), DefaultHasher::new());
    x.hash(&mut hx);
    y.hash(&mut hy);
    assert_eq!(hx.finish(), hy.finish());
    // Different content: unequal.
    let z = WithProvenance::new(8_i32, one);
    assert_ne!(x, z);
    // A BTreeSet keys on content, so the two 7s dedupe to one.
    let set: BTreeSet<WithProvenance<i32>> = [x, y, z].into_iter().collect();
    assert_eq!(set.len(), 2);
}

#[test]
fn map_maps_content_and_carries_provenance_through() {
    let provenance = Provenance::from(Origin::Constructed).with_doc("d");
    let mapped = WithProvenance::new(3_i32, provenance.clone()).map(|n| n * 2);
    assert_eq!(*mapped.get(), 6);
    assert_eq!(mapped.provenance(), &provenance);
}

fn any_provenance() -> impl Strategy<Value = Provenance> {
    let origin = prop_oneof![
        Just(Origin::Constructed),
        "[a-z]{1,3}".prop_map(|s| Origin::Transformed(TransformTag::new(s))),
    ];
    (
        prop::collection::vec(origin, 0..3),
        prop::collection::vec("[a-z]{1,3}", 0..3),
    )
        .prop_map(|(origins, docs)| {
            let mut provenance = Provenance::empty();
            for origin in origins {
                provenance = provenance.merge(Provenance::from(origin));
            }
            for doc in docs {
                provenance = provenance.with_doc(doc);
            }
            provenance
        })
}

proptest! {
    /// The merge is idempotent — a bounded join-semilattice's first law (§6.2, §6.3).
    #[test]
    fn merge_is_idempotent(p in any_provenance()) {
        prop_assert_eq!(p.clone().merge(p.clone()), p);
    }

    /// The merge is commutative.
    #[test]
    fn merge_is_commutative(a in any_provenance(), b in any_provenance()) {
        prop_assert_eq!(a.clone().merge(b.clone()), b.merge(a));
    }

    /// The merge is associative.
    #[test]
    fn merge_is_associative(a in any_provenance(), b in any_provenance(), c in any_provenance()) {
        prop_assert_eq!(a.clone().merge(b.clone()).merge(c.clone()), a.merge(b.merge(c)));
    }

    /// `empty` is the merge identity.
    #[test]
    fn empty_is_the_merge_identity(p in any_provenance()) {
        prop_assert_eq!(Provenance::empty().merge(p.clone()), p.clone());
        prop_assert_eq!(p.clone().merge(Provenance::empty()), p);
    }
}
