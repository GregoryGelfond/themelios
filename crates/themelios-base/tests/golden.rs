//! The human renderer's golden seed corpus (docs/design/base.md §10),
//! reviewed against the rust-analyzer bar. Bless with
//! `GOLDEN_BLESS=1 cargo test -p themelios-base --test golden`, then
//! review the diff before committing: these files are reviewed
//! artifacts, not incidental output.

use std::fs;
use std::path::PathBuf;

use themelios_base::diagnostic::{Diagnostic, DiagnosticId, Label, Severity};
use themelios_base::source::{SourceId, SourceSet};
use themelios_base::span::{ByteOffset, Location, Span};
use themelios_base::view::human;

const UNEXPECTED: DiagnosticId = DiagnosticId::new("syntax", "unexpected-token");

fn check(name: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.txt"));
    if std::env::var_os("GOLDEN_BLESS").is_some() {
        fs::write(&path, actual).expect("golden file writes");
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing golden file {}; bless it and review the rendering",
            path.display()
        )
    });
    assert_eq!(
        actual, expected,
        "rendering diverged from the reviewed golden `{name}`"
    );
}

fn label(source: SourceId, start: u32, end: u32, message: Option<&str>) -> Label {
    Label {
        location: Location {
            source,
            span: Span::new(ByteOffset::new(start), ByteOffset::new(end))
                .expect("ordered endpoints"),
        },
        message: message.map(str::to_owned),
    }
}

fn demo_catalog() -> (SourceSet, SourceId) {
    let mut catalog = SourceSet::new();
    let file = catalog
        .add(
            "demo.lp".to_owned(),
            "p(a).\nq(X) :- r(X)\ns(1..3).\n% done\n".to_owned(),
        )
        .expect("small text admits");
    (catalog, file)
}

fn diagnostic(primary: Label) -> Diagnostic {
    Diagnostic::new(
        UNEXPECTED,
        Severity::Error,
        "expected `.` after the rule body".to_owned(),
        primary,
    )
    .expect("non-empty headline")
}

#[test]
fn single_span() {
    let (catalog, file) = demo_catalog();
    let d = diagnostic(label(file, 14, 18, Some("the rule body ends here")));
    check("single-span", &human(&d, &catalog));
}

#[test]
fn multiple_spans_on_one_line() {
    let (catalog, file) = demo_catalog();
    let d = diagnostic(label(file, 14, 18, Some("this literal"))).with_secondary(label(
        file,
        6,
        10,
        Some("while parsing this head"),
    ));
    check("multiple-spans-one-line", &human(&d, &catalog));
}

#[test]
fn multi_line_span() {
    let (catalog, file) = demo_catalog();
    // Bytes 6..34: from the rule through "% done" — three lines.
    let d = diagnostic(label(file, 6, 34, Some("this whole region")));
    check("multi-line-span", &human(&d, &catalog));
}

#[test]
fn cross_source_secondary() {
    let (mut catalog, file) = demo_catalog();
    let other = catalog
        .add("defs.lp".to_owned(), "r(1).\n".to_owned())
        .expect("small text admits");
    let d = diagnostic(label(file, 14, 18, None)).with_secondary(label(
        other,
        0,
        5,
        Some("defined here"),
    ));
    check("cross-source-secondary", &human(&d, &catalog));
}

#[test]
fn notes_and_helps() {
    let (catalog, file) = demo_catalog();
    let d = diagnostic(label(file, 14, 18, None))
        .with_note("expected because of this rule form".to_owned())
        .with_note("the statement began at line 2".to_owned())
        .with_help("add `.` at the end of the rule".to_owned());
    check("notes-and-helps", &human(&d, &catalog));
}

#[test]
fn unresolvable_source() {
    let (catalog, file) = demo_catalog();
    let d = diagnostic(label(file, 14, 18, None)).with_secondary(label(
        SourceId::new(9),
        0,
        3,
        Some("from here"),
    ));
    check("unresolvable-source", &human(&d, &catalog));
}

/// A catalog breaching the completeness law: name and text resolve,
/// the index does not.
struct MissingIndexCatalog {
    text: String,
}

impl themelios_base::source::Sources for MissingIndexCatalog {
    fn name(&self, _: SourceId) -> Option<&str> {
        Some("partial.lp")
    }
    fn text(&self, _: SourceId) -> Option<&str> {
        Some(&self.text)
    }
    fn line_index(&self, _: SourceId) -> Option<&themelios_base::line::LineIndex> {
        None
    }
}

#[test]
fn missing_facet() {
    let catalog = MissingIndexCatalog {
        text: "p(a).".to_owned(),
    };
    let d = diagnostic(label(SourceId::new(0), 0, 5, None));
    check("missing-facet", &human(&d, &catalog));
}

#[test]
fn span_text_mismatch() {
    let (catalog, file) = demo_catalog();
    // 90..95 is past the text; 15..16 is coherent and still renders.
    let d = diagnostic(label(file, 90, 95, Some("phantom"))).with_secondary(label(
        file,
        15,
        16,
        Some("still renders"),
    ));
    check("span-text-mismatch", &human(&d, &catalog));
}

#[test]
fn embedded_snippet_frame() {
    // An embedded source: the host names it in its own terms, and
    // every coordinate is snippet-relative (base.md §3.3).
    let mut catalog = SourceSet::new();
    let snippet = catalog
        .add(
            "rule! at src/scheduler.rs:41".to_owned(),
            "on(T) :- task(T),\n  not off(T)\n".to_owned(),
        )
        .expect("small text admits");
    let d = diagnostic(label(snippet, 20, 31, Some("negated here")));
    check("embedded-snippet-frame", &human(&d, &catalog));
}

/// A catalog breaching the coherence law: the index predates the
/// text — the one breach the views trust past (base.md §3.4), whose
/// fallback rendering must be a reviewed artifact like every other
/// placeholder.
struct StaleIndexCatalog {
    text: String,
    index: themelios_base::line::LineIndex,
}

impl themelios_base::source::Sources for StaleIndexCatalog {
    fn name(&self, _: SourceId) -> Option<&str> {
        Some("stale.lp")
    }
    fn text(&self, _: SourceId) -> Option<&str> {
        Some(&self.text)
    }
    fn line_index(&self, _: SourceId) -> Option<&themelios_base::line::LineIndex> {
        Some(&self.index)
    }
}

#[test]
fn stale_index() {
    use themelios_base::source::Source;
    // The index knows one long line; the text is now shorter. The
    // renderer places its named placeholder — never a panic, never
    // silence.
    let old = Source::new(SourceId::new(0), "p(a). q(b). r(c). s(d).".to_owned())
        .expect("small text admits");
    let catalog = StaleIndexCatalog {
        text: "p.".to_owned(),
        index: themelios_base::line::LineIndex::of(&old),
    };
    let d = diagnostic(label(SourceId::new(0), 6, 11, Some("was here")));
    check("stale-index", &human(&d, &catalog));
}
