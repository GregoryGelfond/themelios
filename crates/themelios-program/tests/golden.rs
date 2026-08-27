//! The lowering-diagnostics golden corpus (docs/design/program.md §8, §16), reviewed
//! against the rust-analyzer bar (spec §2 item 9). The inputs are the syntax tier's
//! vendored corpus (spec §10.3), re-read by path — no new corpus is vendored; the
//! snapshots under `tests/golden/lowering/` are the reviewed artifacts. A raise of the
//! authority's own programs emits no spurious lowering diagnostic (an empty report), so
//! a snapshot that gains one is a regression a maintainer reads. Bless with
//! `GOLDEN_BLESS=1 cargo test -p themelios-program --test golden`, then review the diff.

use std::fs;
use std::path::PathBuf;

use themelios_base::diagnostic::ToDiagnostic;
use themelios_base::source::{Source, SourceSet};
use themelios_base::view::{canonical_order, human};
use themelios_program::raise::raise;
use themelios_program::render::render;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

/// A corpus input by its path under the syntax tier's corpus, and the golden name its
/// rendered lowering diagnostics are snapshotted under. A diverse spread of the
/// authority's own programs — aggregates, externals, projects, shows, numbers, and a
/// larger planning encoding — lowers clean, so a spurious diagnostic anywhere in the
/// lowering shows; the two seeds below carry a form the grammar admits but the value
/// cannot represent, so their rendered diagnostics are reviewed at the bar.
const INPUTS: &[(&str, &str)] = &[
    ("clingo/app/clingo/tests/lp/aggregates.lp", "aggregates"),
    ("clingo/app/clingo/tests/lp/external.lp", "external"),
    ("clingo/app/clingo/tests/lp/project.lp", "project"),
    ("clingo/app/clingo/tests/lp/show.lp", "show"),
    ("clingo/app/clingo/tests/lp/numbers.lp", "numbers"),
    ("clingo/app/clingo/tests/lp/subset.lp", "subset"),
    ("clingo/app/clingo/tests/lp/istop.lp", "istop"),
    // A numeral beyond the engine's width — a member the value cannot hold (§3.1).
    (
        "seeds/clingo/numeral-overflow-unpinned.lp",
        "numeral-overflow",
    ),
    // A head aggregate whose elements the recovery left incomplete (§8).
    (
        "seeds/clingo/empty-aggregate-elements-in-head.lp",
        "incomplete-head-aggregate",
    ),
];

/// The syntax tier's vendored corpus directory (spec §10.3) — re-read by path.
fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../themelios-syntax/tests/corpus")
}

/// The rendered lowering-diagnostic report of a corpus program: its diagnostics in the
/// shared batch order (base §7.4), each through base's human view (base §6.5) — the
/// report a maintainer reads at the rust-analyzer bar. Empty of diagnostics is a named
/// clean report, so the snapshot is a reviewed artifact either way.
fn lowering_report(name: &str, text: &str) -> String {
    let mut catalog = SourceSet::new();
    let id = catalog
        .add(name.to_owned(), text.to_owned())
        .expect("a corpus input admits");
    let source = Source::new(id, text.to_owned()).expect("a corpus input admits");
    let raised = raise(&parse(&source, Dialect::Clingo));
    let mut diagnostics: Vec<_> = raised
        .diagnostics()
        .iter()
        .map(ToDiagnostic::to_diagnostic)
        .collect();
    diagnostics.sort_by(canonical_order);
    if diagnostics.is_empty() {
        return "no lowering diagnostics\n".to_owned();
    }
    diagnostics
        .iter()
        .map(|diagnostic| human(diagnostic, &catalog))
        .collect()
}

/// The canonical clingo rendering of a corpus program (docs/design/program.md §10) — the
/// reviewed artifact a golden pins, stable because the rendering is canonical. A clean member
/// of the corpus renders without refusal, so a spelling refusal here is itself a regression a
/// maintainer reads.
fn rendering(name: &str, text: &str) -> String {
    let source = Source::new(themelios_base::source::SourceId::new(0), text.to_owned())
        .expect("a corpus input admits");
    let raised = raise(&parse(&source, Dialect::Clingo));
    render(raised.program(), Dialect::Clingo)
        .unwrap_or_else(|refusal| panic!("the corpus program `{name}` renders: {refusal}"))
}

/// A spread of the authority's own programs whose canonical renderings are reviewed snapshots
/// (§10) — aggregates and their guards, the directives, the arithmetic of numbers.
const RENDER_INPUTS: &[(&str, &str)] = &[
    ("clingo/app/clingo/tests/lp/aggregates.lp", "aggregates"),
    ("clingo/app/clingo/tests/lp/external.lp", "external"),
    ("clingo/app/clingo/tests/lp/project.lp", "project"),
    ("clingo/app/clingo/tests/lp/show.lp", "show"),
    ("clingo/app/clingo/tests/lp/numbers.lp", "numbers"),
];

/// Compare a rendered report to its reviewed snapshot under `subdirectory`, or rewrite it
/// under the bless toggle — the reviewed-artifact discipline base's golden corpus keeps
/// (base §10).
fn check(subdirectory: &str, name: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(subdirectory)
        .join(format!("{name}.txt"));
    if std::env::var_os("GOLDEN_BLESS").is_some() {
        fs::create_dir_all(path.parent().expect("a golden parent directory"))
            .expect("the golden directory is writable");
        fs::write(&path, actual).expect("the golden file writes");
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
        "the output diverged from the reviewed golden `{name}`"
    );
}

#[test]
fn the_authoritys_programs_lower_without_spurious_diagnostics() {
    for (relative, name) in INPUTS {
        let text = fs::read_to_string(corpus_dir().join(relative))
            .unwrap_or_else(|error| panic!("corpus input `{relative}` reads: {error}"));
        check("lowering", name, &lowering_report(name, &text));
    }
}

#[test]
fn the_authoritys_programs_render_to_their_reviewed_snapshots() {
    for (relative, name) in RENDER_INPUTS {
        let text = fs::read_to_string(corpus_dir().join(relative))
            .unwrap_or_else(|error| panic!("corpus input `{relative}` reads: {error}"));
        check("render", name, &rendering(name, &text));
    }
}
