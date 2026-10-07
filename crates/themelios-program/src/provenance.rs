//! In-node provenance (docs/design/program.md §6): where a structural node was
//! parsed from, that it was constructed, the transformation that produced it, and
//! the tool and modeler annotations attached to it. The carrier's identity **erases**
//! provenance (§6.2), so equality is up to it and a set dedupes by content, and the
//! merge is a bounded join-semilattice (§6.3) — nothing lost, nothing fabricated.
//! `Term` and `Symbol` are not wrapped: they are the clean, origin-free algebra the
//! depth discipline walks (§6.1).

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt;
use std::hash::{Hash, Hasher};

use themelios_base::span::Location;

/// A structural node and its provenance (§6.2). Identity is the content's; provenance
/// is erased — so equality is up to provenance (§5) and a set dedupes by content. This
/// is the one place the erasure is written, so it cannot drift per node: `PartialEq`,
/// `Eq`, `PartialOrd`, `Ord`, and `Hash` read `value` alone, while `Clone` and `Debug`
/// (derived) carry both fields.
#[derive(Clone, Debug)]
pub struct WithProvenance<T> {
    value: T,
    provenance: Provenance,
}

impl<T> WithProvenance<T> {
    /// A node with the given content and provenance (§6.2).
    pub fn new(value: T, provenance: Provenance) -> WithProvenance<T> {
        WithProvenance { value, provenance }
    }

    /// A node built through the constructors, its origin `Constructed` (§6.2, §7).
    pub fn constructed(value: T) -> WithProvenance<T> {
        WithProvenance::new(value, Provenance::from(Origin::Constructed))
    }

    /// A constructed node with a documentation block attached in one call: its origin is
    /// `Constructed` (§6.2, §7) and `doc` is inserted as a single element of the doc
    /// annotation set (§6.2). A multi-line block passed as one newline-joined string is
    /// thus one element with the author's line order intact inside it, and it unions and
    /// dedupes by content on merge (§6.3). This is `constructed` plus `Provenance::with_doc`,
    /// so a consumer building a documented node need not thread a `Provenance` by hand.
    pub fn constructed_with_doc(value: T, doc: impl Into<String>) -> WithProvenance<T> {
        WithProvenance::new(value, Provenance::from(Origin::Constructed).with_doc(doc))
    }

    /// The content (§6.2).
    pub fn get(&self) -> &T {
        &self.value
    }

    /// The provenance — origins and annotations (§6.2).
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// The owned content, the complement to `get`'s borrow (§6.2).
    pub fn into_value(self) -> T {
        self.value
    }

    /// The value and its provenance, owned — the door to a rewrite that rebuilds a node it owns
    /// (§9.1). A merge unions provenance through `absorb_later` and `absorb_earlier` (§6.3).
    /// Crate-internal. O(1).
    pub(crate) fn into_parts(self) -> (T, Provenance) {
        (self.value, self.provenance)
    }

    /// Fold a later content-equal node into this one, as the set merge does (§6.3): the
    /// later node's content, with its nested provenance, and this node's provenance unioned
    /// with the later one's. Crate-internal; O(the union).
    pub(crate) fn absorb_later(&mut self, later: WithProvenance<T>) {
        let accumulated = std::mem::take(&mut self.provenance);
        self.value = later.value;
        self.provenance = accumulated.merge(later.provenance);
    }

    /// Fold an earlier content-equal node into this one, as the set merge does (§6.3): this
    /// node's content stays, with its nested provenance, and its provenance becomes the
    /// earlier one's unioned with its own. Crate-internal; O(the union).
    pub(crate) fn absorb_earlier(&mut self, earlier: WithProvenance<T>) {
        let newer = std::mem::take(&mut self.provenance);
        self.provenance = earlier.provenance.merge(newer);
    }

    /// Rewrite the content, carrying the provenance through unchanged — the transform
    /// surface's workhorse (§6.2, §9.1).
    #[must_use]
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> WithProvenance<U> {
        WithProvenance {
            value: f(self.value),
            provenance: self.provenance,
        }
    }
}

// The erasure, written once (§6.2): the identity traits read `value` alone, so two
// carriers of equal content but different provenance are equal, order-equal, and
// hash-equal, and a `BTreeSet<WithProvenance<T>>` keys on content.
impl<T: PartialEq> PartialEq for WithProvenance<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}
impl<T: Eq> Eq for WithProvenance<T> {}
impl<T: Ord> PartialOrd for WithProvenance<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<T: Ord> Ord for WithProvenance<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}
impl<T: Hash> Hash for WithProvenance<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

/// A node's provenance: a set of origin facts and a set of annotations, merged by
/// union (§6.2). Empty is the identity; merge is idempotent, commutative, and
/// associative — a bounded join-semilattice — which is what lets a content-equal
/// collapse (§5) *union* both nodes' provenance rather than keep one arbitrarily.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Provenance {
    origins: Origins,
    annotations: Annotations,
}

impl Provenance {
    /// The empty provenance — the merge identity (also `Default`) (§6.2).
    pub fn empty() -> Provenance {
        Provenance::default()
    }

    /// The origin facts, in `Origin` order (§6.2).
    pub fn origins(&self) -> impl Iterator<Item = &Origin> {
        self.origins.iter()
    }

    /// The annotations (§6.2).
    pub fn annotations(&self) -> &Annotations {
        &self.annotations
    }

    /// Attach a documentation string (the raise's `%!` doc comment, §8). `with_doc` is the
    /// annotation builder §6.2 makes explicit; the label, reference, and trace annotations
    /// merge (below) but have no builder here.
    #[must_use]
    pub fn with_doc(mut self, doc: impl Into<String>) -> Provenance {
        self.annotations.kinds_mut().doc.insert(doc.into());
        self
    }

    /// The union of two provenances — a join-semilattice (§6.3): the origin sets and
    /// each annotation kind unioned, nothing lost and nothing fabricated.
    #[must_use]
    pub fn merge(self, other: Provenance) -> Provenance {
        Provenance {
            origins: self.origins.union(other.origins),
            annotations: self.annotations.merge(other.annotations),
        }
    }
}

/// A single origin fact left in a provenance (§6.2). `Origin` is public, so the
/// construction of a provenance from one origin is the `From<Origin>` below — the
/// surface §6.2 leaves implicit, named here.
impl From<Origin> for Provenance {
    fn from(origin: Origin) -> Provenance {
        Provenance {
            origins: Origins::One(origin),
            annotations: Annotations::default(),
        }
    }
}

/// A provenance's origin facts: a set (§6.2) that holds a lone fact inline. Nearly every
/// node carries exactly one origin — its parsed span, or the constructed mark — so the
/// common provenance allocates nothing (§6.3); a union of two or more facts is an ordered
/// set on the heap. Each size has one form — no fact, one, or a set of at least two — so
/// structural equality is set equality.
#[derive(Clone, PartialEq, Eq, Default)]
enum Origins {
    #[default]
    None,
    One(Origin),
    Many(BTreeSet<Origin>),
}

impl Origins {
    /// The facts, in `Origin` order. O(1) per fact.
    fn iter(&self) -> impl Iterator<Item = &Origin> {
        let (one, many) = match self {
            Origins::None => (None, None),
            Origins::One(origin) => (Some(origin), None),
            Origins::Many(set) => (None, Some(set.iter())),
        };
        one.into_iter().chain(many.into_iter().flatten())
    }

    /// The union, moving both: the smaller side is inserted into the larger, so a run of
    /// n merges into one accumulating set costs O(n log n) (§6.3).
    fn union(self, other: Origins) -> Origins {
        match (self, other) {
            (Origins::None, origins) | (origins, Origins::None) => origins,
            (Origins::One(left), Origins::One(right)) => {
                if left == right {
                    Origins::One(left)
                } else {
                    Origins::Many(BTreeSet::from([left, right]))
                }
            }
            (Origins::One(origin), Origins::Many(mut set))
            | (Origins::Many(mut set), Origins::One(origin)) => {
                set.insert(origin);
                Origins::Many(set)
            }
            (Origins::Many(mut larger), Origins::Many(mut smaller)) => {
                if larger.len() < smaller.len() {
                    std::mem::swap(&mut larger, &mut smaller);
                }
                larger.extend(smaller);
                Origins::Many(larger)
            }
        }
    }
}

/// Rendered as the set it is, as the standard ordered set renders.
impl fmt::Debug for Origins {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

/// Where a node came from (§6.2).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Origin {
    /// A span in a source (base §4.3) — the blame workhorse.
    Parsed(Location),
    /// Built through the constructors (§7).
    Constructed,
    /// Produced by a named transformation (§9).
    Transformed(TransformTag),
}

/// The name of a transformation that produced a node (§9.1).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TransformTag(String);

impl TransformTag {
    /// A tag naming a transformation. Named here as the construction surface a rewrite
    /// (§9.1) writes; §6.2 leaves it implicit.
    pub fn new(name: impl Into<String>) -> TransformTag {
        TransformTag(name.into())
    }

    /// The transformation's name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Tool and modeler annotations (§6.2): a documentation string (from a `%!` doc
/// comment, §8), a label, a reference, and a trace directive an explanation tool
/// attaches (§2). Each kind is a set, unioned on merge. Nearly every node carries none,
/// so the kinds are boxed only once one holds a string: an unannotated node's
/// annotations are a null pointer (§6.3). The box is there only while some kind holds a
/// string — the builder boxes as it inserts, and a merge keeps either side's box — so
/// each annotation has one form and structural equality is set equality.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Annotations {
    kinds: Option<Box<Kinds>>,
}

/// The four annotation kinds of an annotated node (§6.2).
#[derive(Clone, PartialEq, Eq, Default)]
struct Kinds {
    doc: BTreeSet<String>,
    label: BTreeSet<String>,
    reference: BTreeSet<String>,
    trace: BTreeSet<String>,
}

impl Annotations {
    /// The documentation strings (§6.2).
    pub fn doc(&self) -> impl Iterator<Item = &str> {
        self.strings(|kinds| &kinds.doc)
    }

    /// The labels (§6.2).
    pub fn label(&self) -> impl Iterator<Item = &str> {
        self.strings(|kinds| &kinds.label)
    }

    /// The references (§6.2).
    pub fn reference(&self) -> impl Iterator<Item = &str> {
        self.strings(|kinds| &kinds.reference)
    }

    /// The trace directives (§6.2).
    pub fn trace(&self) -> impl Iterator<Item = &str> {
        self.strings(|kinds| &kinds.trace)
    }

    /// The union of two annotation sets, each kind unioned (§6.3).
    #[must_use]
    pub fn merge(self, other: Annotations) -> Annotations {
        let kinds = match (self.kinds, other.kinds) {
            (None, kinds) | (kinds, None) => kinds,
            (Some(mut left), Some(right)) => {
                let Kinds {
                    doc,
                    label,
                    reference,
                    trace,
                } = *right;
                left.doc.extend(doc);
                left.label.extend(label);
                left.reference.extend(reference);
                left.trace.extend(trace);
                Some(left)
            }
        };
        Annotations { kinds }
    }

    /// One kind's strings, in order; none when the node is unannotated.
    fn strings(&self, kind: impl Fn(&Kinds) -> &BTreeSet<String>) -> impl Iterator<Item = &str> {
        self.kinds
            .as_deref()
            .map(kind)
            .into_iter()
            .flatten()
            .map(String::as_str)
    }

    /// The kinds, boxed on first use, for a builder that inserts into them at once — the
    /// box is never left empty.
    fn kinds_mut(&mut self) -> &mut Kinds {
        self.kinds.get_or_insert_with(Box::default)
    }
}

/// Rendered with every kind as the set it is, boxed or not.
impl fmt::Debug for Annotations {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        /// One kind, rendered as the standard ordered set renders.
        struct Set<'a>(Option<&'a BTreeSet<String>>);
        impl fmt::Debug for Set<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_set().entries(self.0.into_iter().flatten()).finish()
            }
        }
        let kinds = self.kinds.as_deref();
        f.debug_struct("Annotations")
            .field("doc", &Set(kinds.map(|kinds| &kinds.doc)))
            .field("label", &Set(kinds.map(|kinds| &kinds.label)))
            .field("reference", &Set(kinds.map(|kinds| &kinds.reference)))
            .field("trace", &Set(kinds.map(|kinds| &kinds.trace)))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{Annotations, Origin, Provenance, WithProvenance};

    #[test]
    fn each_annotation_kind_reads_its_own_strings() {
        // The four kinds are four sets (§6.2): each accessor reads its own, so a string
        // written to one kind is read from that kind alone. Only `with_doc` writes from
        // outside this module; the others are written here through the kinds' one builder.
        let mut annotations = Annotations::default();
        let kinds = annotations.kinds_mut();
        kinds.doc.insert("d".to_owned());
        kinds.label.insert("l".to_owned());
        kinds.reference.insert("r".to_owned());
        kinds.trace.insert("t".to_owned());
        assert_eq!(annotations.doc().collect::<Vec<_>>(), ["d"]);
        assert_eq!(annotations.label().collect::<Vec<_>>(), ["l"]);
        assert_eq!(annotations.reference().collect::<Vec<_>>(), ["r"]);
        assert_eq!(annotations.trace().collect::<Vec<_>>(), ["t"]);
    }

    #[test]
    fn the_kinds_a_provenance_never_set_read_empty() {
        // Only `with_doc` builds an annotation (§6.2): the other kinds read empty beside it,
        // and every kind reads empty on an unannotated node.
        let documented = Provenance::from(Origin::Constructed).with_doc("d");
        let annotations = documented.annotations();
        assert_eq!(annotations.doc().collect::<Vec<_>>(), ["d"]);
        assert_eq!(annotations.label().count(), 0);
        assert_eq!(annotations.reference().count(), 0);
        assert_eq!(annotations.trace().count(), 0);
        let bare = Provenance::from(Origin::Constructed);
        assert_eq!(bare.annotations().doc().count(), 0);
        assert_eq!(bare.annotations().trace().count(), 0);
    }

    #[test]
    fn constructed_with_doc_records_the_constructed_origin() {
        // The origin half of the one-call ctor (§6.2, §7): a lone `Constructed` fact, as
        // `constructed` gives — present alongside the doc, not in place of it.
        let node = WithProvenance::constructed_with_doc(1, "d");
        let origins: Vec<&Origin> = node.provenance().origins().collect();
        assert_eq!(origins, [&Origin::Constructed]);
    }

    #[test]
    fn constructed_with_doc_stores_the_doc_string() {
        // The doc half, present alongside the origin — both, not one or the other (§6.2).
        let node = WithProvenance::constructed_with_doc(1, "d");
        let docs: Vec<&str> = node.provenance().annotations().doc().collect();
        assert_eq!(docs, ["d"]);
    }

    #[test]
    fn constructed_with_doc_keeps_a_multiline_doc_as_one_ordered_element() {
        // A multi-line block is one newline-joined element (§6.2): the set holds one
        // string, not two lines, with the author's line order intact inside it.
        let node = WithProvenance::constructed_with_doc(1, "line one\nline two");
        let docs: Vec<&str> = node.provenance().annotations().doc().collect();
        assert_eq!(docs, ["line one\nline two"]);
    }

    #[test]
    fn equal_nodes_with_equal_docs_are_one_content() {
        // The erasure (§6.2): content-equal carriers are equal, so a set holds one.
        let a = WithProvenance::constructed_with_doc(1, "d");
        let b = WithProvenance::constructed_with_doc(1, "d");
        assert_eq!(a, b);
    }

    #[test]
    fn merging_the_same_doc_dedupes_to_one_element() {
        // The union-merge dedupes by content (§6.3): two `"d"`s collapse to a single one.
        let a = WithProvenance::constructed_with_doc(1, "d");
        let b = WithProvenance::constructed_with_doc(1, "d");
        let merged = a.provenance().clone().merge(b.provenance().clone());
        let docs: Vec<&str> = merged.annotations().doc().collect();
        assert_eq!(docs, ["d"]);
    }

    #[test]
    fn merging_provenances_is_commutative() {
        // Merge is commutative (§6.3): the order of the two provenances does not matter.
        let a = WithProvenance::constructed_with_doc(1, "one");
        let b = WithProvenance::constructed_with_doc(1, "two");
        let ab = a.provenance().clone().merge(b.provenance().clone());
        let ba = b.provenance().clone().merge(a.provenance().clone());
        assert_eq!(ab, ba);
    }

    #[test]
    fn merging_different_docs_keeps_both() {
        // The union loses nothing (§6.3): distinct docs on equal content both survive.
        let a = WithProvenance::constructed_with_doc(1, "one");
        let b = WithProvenance::constructed_with_doc(1, "two");
        let merged = a.provenance().clone().merge(b.provenance().clone());
        let docs: Vec<&str> = merged.annotations().doc().collect();
        assert_eq!(docs, ["one", "two"]);
    }
}
