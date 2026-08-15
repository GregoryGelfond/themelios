//! The `Sources` law checker, exercised against both outcomes
//! (docs/design/base.md §10): the shipped catalog passes by
//! construction; deliberately incomplete and incoherent catalogs
//! fail, naming the breach.

use themelios_base::line::LineIndex;
use themelios_base::source::{
    Source, SourceFacet, SourceId, SourceSet, Sources, SourcesLawViolation, check_sources_laws,
};

#[test]
fn shipped_catalog_satisfies_the_laws() {
    let mut catalog = SourceSet::new();
    let first = catalog
        .add("demo.lp".to_owned(), "p(a).\n".to_owned())
        .expect("small text admits");
    let second = catalog
        .add("other.lp".to_owned(), "q(🦀).".to_owned())
        .expect("small text admits");
    assert_eq!(check_sources_laws(&catalog, &[first, second]), vec![]);
    assert_eq!(catalog.name(first), Some("demo.lp"));
    assert_eq!(catalog.text(second), Some("q(🦀)."));
    assert!(catalog.line_index(first).is_some());
    // Unknown identities answer None — a refusal, never a panic.
    let unknown = SourceId::new(99);
    assert_eq!(catalog.name(unknown), None);
    assert_eq!(catalog.text(unknown), None);
    assert!(catalog.line_index(unknown).is_none());
}

#[test]
fn ids_are_minted_sequentially() {
    let mut catalog = SourceSet::new();
    let first = catalog
        .add("a".to_owned(), String::new())
        .expect("empty text admits");
    let second = catalog
        .add("b".to_owned(), String::new())
        .expect("empty text admits");
    assert_eq!(first, SourceId::new(0));
    assert_eq!(second, SourceId::new(1));
}

/// A catalog that resolves name and text but no index — a
/// completeness breach.
struct MissingIndex {
    text: String,
}

impl Sources for MissingIndex {
    fn name(&self, _: SourceId) -> Option<&str> {
        Some("partial.lp")
    }
    fn text(&self, _: SourceId) -> Option<&str> {
        Some(&self.text)
    }
    fn line_index(&self, _: SourceId) -> Option<&LineIndex> {
        None
    }
}

#[test]
fn an_incomplete_catalog_is_named_facet_by_facet() {
    let catalog = MissingIndex {
        text: "p.".to_owned(),
    };
    let id = SourceId::new(0);
    assert_eq!(
        check_sources_laws(&catalog, &[id]),
        vec![SourcesLawViolation::Incomplete {
            id,
            missing: SourceFacet::Index,
        }]
    );
}

/// A catalog whose index was built from an earlier version of the
/// text — the stale-cache breach, the one route to a misplaced caret
/// that no view can see (base.md §3.4).
struct StaleIndex {
    text: String,
    index: LineIndex,
}

impl StaleIndex {
    fn new() -> StaleIndex {
        let old = Source::new(SourceId::new(0), "one line".to_owned()).expect("small text admits");
        StaleIndex {
            text: "two\nlines".to_owned(),
            index: LineIndex::of(&old),
        }
    }
}

impl Sources for StaleIndex {
    fn name(&self, _: SourceId) -> Option<&str> {
        Some("stale.lp")
    }
    fn text(&self, _: SourceId) -> Option<&str> {
        Some(&self.text)
    }
    fn line_index(&self, _: SourceId) -> Option<&LineIndex> {
        Some(&self.index)
    }
}

#[test]
fn an_incoherent_index_is_caught_by_rederivation() {
    let catalog = StaleIndex::new();
    let id = SourceId::new(0);
    assert_eq!(
        check_sources_laws(&catalog, &[id]),
        vec![SourcesLawViolation::IncoherentIndex { id }]
    );
}

#[test]
fn an_unknown_id_breaches_nothing() {
    let catalog = SourceSet::new();
    assert_eq!(check_sources_laws(&catalog, &[SourceId::new(3)]), vec![]);
}
