//! In-node provenance (docs/design/program.md §6): where a structural node was
//! parsed from, that it was constructed, the transformation that produced it, and
//! the tool and modeler annotations attached to it. The carrier's identity **erases**
//! provenance (§6.2), so equality is up to it and a set dedupes by content, and the
//! merge is a bounded join-semilattice (§6.3) — nothing lost, nothing fabricated.
//! `Term` and `Symbol` are not wrapped: they are the clean, origin-free algebra the
//! depth discipline walks (§6.1).

use std::cmp::Ordering;
use std::collections::BTreeSet;
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
    origins: BTreeSet<Origin>,
    annotations: Annotations,
}

impl Provenance {
    /// The empty provenance — the merge identity (also `Default`) (§6.2).
    pub fn empty() -> Provenance {
        Provenance::default()
    }

    /// The origin facts (§6.2).
    pub fn origins(&self) -> impl Iterator<Item = &Origin> {
        self.origins.iter()
    }

    /// The annotations (§6.2).
    pub fn annotations(&self) -> &Annotations {
        &self.annotations
    }

    /// Attach a documentation string (the raise's `%!` doc comment, §8). Named here
    /// as the construction surface §6.2 leaves implicit; the other annotation kinds'
    /// builders land with the consumers that write them.
    #[must_use]
    pub fn with_doc(mut self, doc: impl Into<String>) -> Provenance {
        self.annotations.doc.insert(doc.into());
        self
    }

    /// The union of two provenances — a join-semilattice (§6.3): the origin sets and
    /// each annotation kind unioned, nothing lost and nothing fabricated.
    #[must_use]
    pub fn merge(mut self, other: Provenance) -> Provenance {
        self.origins.extend(other.origins);
        self.annotations = self.annotations.merge(other.annotations);
        self
    }
}

/// A single origin fact left in a provenance (§6.2). `Origin` is public, so the
/// construction of a provenance from one origin is the `From<Origin>` below — the
/// surface §6.2 leaves implicit, named here.
impl From<Origin> for Provenance {
    fn from(origin: Origin) -> Provenance {
        let mut origins = BTreeSet::new();
        origins.insert(origin);
        Provenance {
            origins,
            annotations: Annotations::default(),
        }
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
/// attaches (§2). Each kind is a set, unioned on merge.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Annotations {
    doc: BTreeSet<String>,
    label: BTreeSet<String>,
    reference: BTreeSet<String>,
    trace: BTreeSet<String>,
}

impl Annotations {
    /// The documentation strings (§6.2).
    pub fn doc(&self) -> impl Iterator<Item = &str> {
        self.doc.iter().map(String::as_str)
    }

    /// The labels (§6.2).
    pub fn label(&self) -> impl Iterator<Item = &str> {
        self.label.iter().map(String::as_str)
    }

    /// The references (§6.2).
    pub fn reference(&self) -> impl Iterator<Item = &str> {
        self.reference.iter().map(String::as_str)
    }

    /// The trace directives (§6.2).
    pub fn trace(&self) -> impl Iterator<Item = &str> {
        self.trace.iter().map(String::as_str)
    }

    /// The union of two annotation sets, each kind unioned (§6.3).
    #[must_use]
    pub fn merge(mut self, other: Annotations) -> Annotations {
        self.doc.extend(other.doc);
        self.label.extend(other.label);
        self.reference.extend(other.reference);
        self.trace.extend(other.trace);
        self
    }
}
