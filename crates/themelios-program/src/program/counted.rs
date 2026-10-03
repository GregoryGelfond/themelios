//! The counted collections (docs/design/program.md §4.4): the elements of a choice, a set
//! aggregate, and a theory atom, held as the authority counts them — a repeat the grounder
//! keys by content is one element, a repeat it numbers by occurrence is another.

use std::cmp::Ordering;

use crate::provenance::WithProvenance;

/// Whether a repeat of an element is the same element (`ByContent`) or another
/// (`ByOccurrence`) — the identity a counted collection applies (§4.4). It is the
/// authority's: an element over an atom, under any default negation, by content; over a
/// comparison or a boolean constant by occurrence; a theory element by occurrence (§4.9).
/// Public because a consumer that evaluates the counting itself — a native engine behind the
/// solve contract, or a transformation that must know whether a repeat it builds will merge —
/// reads the rule here rather than re-deriving it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Identity {
    /// A repeat is the same element: the authority keys it by its content.
    ByContent,
    /// A repeat is another element: the authority numbers it by its occurrence.
    ByOccurrence,
}

/// An element a counted collection holds: one that states its own identity (§4.4).
pub(crate) trait Identified: Ord {
    /// How a repeat of this element counts.
    fn identity(&self) -> Identity;
}

/// The elements of a choice, a set aggregate, or a theory atom (§4.4) — a carrier, its
/// entries in `Ord` order, each with its own provenance. Built only by
/// [`Counted::from_elements`]; there is no incremental insert. Equality, order, and hashing
/// read the entries in order: multiset equality over the elements.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) struct Counted<T> {
    entries: Vec<WithProvenance<T>>,
}

impl<T: Identified> Counted<T> {
    /// The one construction door (§4.4): a stable sort into `Ord` order, then one adjacent pass
    /// that joins a by-content repeat to its equal neighbour — the later entry's value kept,
    /// as a set's merge keeps it (§6.3), and the provenances unioned by move — and leaves a
    /// by-occurrence repeat as an entry of its own, content-equal entries keeping the order
    /// they were written or built in. `O(n log n)` comparisons for `n` elements, and each
    /// union `O(m log k)` for `m` origins joining `k`.
    pub(crate) fn from_elements(
        elements: impl IntoIterator<Item = WithProvenance<T>>,
    ) -> Counted<T> {
        let mut sorted: Vec<WithProvenance<T>> = elements.into_iter().collect();
        // Stable: content-equal entries keep their written order, which only their
        // provenance observes (§5.1).
        sorted.sort();
        let mut entries: Vec<WithProvenance<T>> = Vec::with_capacity(sorted.len());
        for entry in sorted {
            match entries.pop() {
                Some(earlier)
                    if entry.get().identity() == Identity::ByContent
                        && earlier.get() == entry.get() =>
                {
                    let (_, accumulated) = earlier.into_parts();
                    let (value, provenance) = entry.into_parts();
                    entries.push(WithProvenance::new(value, accumulated.merge(provenance)));
                }
                Some(earlier) => {
                    entries.push(earlier);
                    entries.push(entry);
                }
                None => entries.push(entry),
            }
        }
        Counted { entries }
    }
}

impl<T> Counted<T> {
    /// The entries, in `Ord` order, each with its provenance. O(1) to begin.
    pub(crate) fn iter(&self) -> std::slice::Iter<'_, WithProvenance<T>> {
        self.entries.iter()
    }

    /// The entries, owned, in `Ord` order — for a door that rebuilds them through
    /// [`Counted::from_elements`]. O(1) to begin.
    pub(crate) fn into_entries(self) -> std::vec::IntoIter<WithProvenance<T>> {
        self.entries.into_iter()
    }
}

impl<T> Default for Counted<T> {
    /// No elements.
    fn default() -> Counted<T> {
        Counted {
            entries: Vec::new(),
        }
    }
}

impl<T: Ord> PartialOrd for Counted<T> {
    fn partial_cmp(&self, other: &Counted<T>) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Ord> Ord for Counted<T> {
    /// The entries compared in order — the order a `BTreeSet` of the same distinct elements
    /// has, so a program with no surviving repeat orders as before (§5.2).
    fn cmp(&self, other: &Counted<T>) -> Ordering {
        self.entries.cmp(&other.entries)
    }
}

#[cfg(test)]
mod tests {
    use super::{Counted, Identified, Identity};
    use crate::provenance::{Origin, Provenance, TransformTag, WithProvenance};
    use proptest::prelude::*;

    /// A probe element: `Kept` counts by occurrence, `Merged` by content.
    #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
    enum Probe {
        Kept(u8),
        Merged(u8),
    }

    impl Identified for Probe {
        fn identity(&self) -> Identity {
            match self {
                Probe::Kept(_) => Identity::ByOccurrence,
                Probe::Merged(_) => Identity::ByContent,
            }
        }
    }

    fn tagged(probe: Probe, tag: &str) -> WithProvenance<Probe> {
        WithProvenance::new(
            probe,
            Provenance::from(Origin::Transformed(TransformTag::new(tag))),
        )
    }

    fn tags(entry: &WithProvenance<Probe>) -> Vec<String> {
        entry
            .provenance()
            .origins()
            .filter_map(|origin| match origin {
                Origin::Transformed(tag) => Some(tag.as_str().to_owned()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn content_equal_kept_entries_keep_their_written_order() {
        let counted = Counted::from_elements([
            tagged(Probe::Kept(1), "first"),
            tagged(Probe::Kept(0), "other"),
            tagged(Probe::Kept(1), "second"),
        ]);
        let order: Vec<Vec<String>> = counted.iter().map(tags).collect();
        assert_eq!(order, vec![vec!["other"], vec!["first"], vec!["second"]]);
    }

    #[test]
    fn a_merged_repeat_unions_every_occurrence_s_provenance() {
        let counted = Counted::from_elements([
            tagged(Probe::Merged(1), "first"),
            tagged(Probe::Merged(1), "second"),
            tagged(Probe::Merged(1), "third"),
        ]);
        let entries: Vec<&WithProvenance<Probe>> = counted.iter().collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(tags(entries[0]), vec!["first", "second", "third"]);
    }

    fn probe() -> impl Strategy<Value = Probe> {
        prop_oneof![
            (0u8..4).prop_map(Probe::Kept),
            (0u8..4).prop_map(Probe::Merged)
        ]
    }

    proptest! {
        /// Against a naive model: a by-occurrence element appears once per occurrence, a
        /// by-content element once, and the order of the input never matters.
        #[test]
        fn the_entries_are_the_naive_count(mut probes in proptest::collection::vec(probe(), 0..24)) {
            let counted = Counted::from_elements(probes.iter().cloned().map(WithProvenance::constructed));
            let mut expected: Vec<Probe> = Vec::new();
            probes.sort();
            for probe in &probes {
                let repeat = matches!(probe, Probe::Merged(_)) && expected.last() == Some(probe);
                if !repeat {
                    expected.push(probe.clone());
                }
            }
            let actual: Vec<Probe> = counted.iter().map(|entry| entry.get().clone()).collect();
            prop_assert_eq!(actual, expected);
            probes.reverse();
            let reversed = Counted::from_elements(probes.into_iter().map(WithProvenance::constructed));
            prop_assert_eq!(reversed, counted);
        }
    }
}
