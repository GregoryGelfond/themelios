//! CI shape assertions (docs/design/base.md §10): complexity shape
//! only, held by median-of-five wall-clock ratios with tolerances
//! wide enough for any CI machine. What they prove: the claimed
//! class (a quadratic `of`, a linear-scan `position`, a
//! labels-squared `human` all fail loudly). What they cannot prove:
//! absolute speed — that lives in the out-of-band benches.
//!
//! The design names criterion for the shape claim; the shape is held
//! here instead because it must run under the test gate, which
//! criterion's benches do not, and criterion measures rather than
//! asserts — it holds the absolute numbers out-of-band
//! (`benches/scaling.rs`).

use std::time::Instant;

use themelios_base::diagnostic::{Diagnostic, DiagnosticId, Label, Severity};
use themelios_base::line::{ColumnEncoding, LineIndex};
use themelios_base::source::{Source, SourceId, SourceSet};
use themelios_base::span::{ByteOffset, Location, Span};
use themelios_base::view::human;

const LINE: &str = "p(a). % é🦀 comment\n";

/// The data-size ratio between the small and large cases.
const SIZE_RATIO: u128 = 16;
/// A linear claim at SIZE_RATIO may cost at most this factor:
/// fourfold noise headroom above linear (x16) and fourfold
/// separation below quadratic (x256) — real margin in both
/// directions.
const LINEAR_CEILING: u128 = SIZE_RATIO * 4;
/// A logarithmic claim across a 64x data ratio may cost at most this
/// factor — logarithmic is ~1.4x; linear (x64) fails.
const LOG_CEILING: u128 = 8;

fn text_of(bytes: usize) -> String {
    LINE.repeat(bytes / LINE.len() + 1)
}

fn admitted(bytes: usize) -> Source {
    Source::new(SourceId::new(0), text_of(bytes)).expect("test text admits")
}

fn median_nanos(mut work: impl FnMut()) -> u128 {
    let mut samples = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        work();
        samples.push(start.elapsed().as_nanos());
    }
    samples.sort_unstable();
    samples[2].max(1)
}

#[test]
fn line_index_construction_is_linear_in_the_text() {
    let small_source = admitted(256 * 1024);
    let big_source = admitted(256 * 1024 * SIZE_RATIO as usize);
    let small = median_nanos(|| {
        std::hint::black_box(LineIndex::of(&small_source));
    });
    let big = median_nanos(|| {
        std::hint::black_box(LineIndex::of(&big_source));
    });
    assert!(
        big < small * LINEAR_CEILING,
        "LineIndex::of scaled {small}ns -> {big}ns over x{SIZE_RATIO} \
         data; the linear shape allows at most x{LINEAR_CEILING}"
    );
}

#[test]
fn position_and_offset_are_logarithmic() {
    let queries = 4096usize;
    let run = |bytes: usize| {
        let source = admitted(bytes);
        let index = LineIndex::of(&source);
        let len = source.end().get() as usize;
        let offsets: Vec<ByteOffset> = (0..queries)
            .map(|i| (i * (len / queries)) / LINE.len() * LINE.len())
            .map(|raw| ByteOffset::new(raw as u32))
            .collect();
        median_nanos(move || {
            for &offset in &offsets {
                let position = index
                    .position(offset, ColumnEncoding::Utf16Units)
                    .expect("boundary offsets position");
                std::hint::black_box(
                    index
                        .offset(position, ColumnEncoding::Utf16Units)
                        .expect("round trip"),
                );
            }
        })
    };
    // 64x the indexed text, the same number of queries.
    let small = run(64 * 1024);
    let big = run(4096 * 1024);
    assert!(
        big < small * LOG_CEILING,
        "position/offset scaled {small}ns -> {big}ns over x64 data; \
         the logarithmic shape allows at most x{LOG_CEILING}"
    );
}

#[test]
fn human_is_linear_in_rendered_output() {
    let mut catalog = SourceSet::new();
    let file = catalog
        .add("shape.lp".to_owned(), text_of(512 * LINE.len()))
        .expect("test text admits");
    let label_on_line = |line: u32| Label {
        location: Location {
            source: file,
            span: Span::new(
                ByteOffset::new(line * LINE.len() as u32),
                ByteOffset::new(line * LINE.len() as u32 + 4),
            )
            .expect("ordered endpoints"),
        },
        message: Some("here".to_owned()),
    };
    let build = |labels: u32| {
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::new("syntax", "unexpected-token"),
            Severity::Error,
            "shape".to_owned(),
            label_on_line(0),
        )
        .expect("non-empty headline");
        for line in 1..labels {
            diagnostic = diagnostic.with_secondary(label_on_line(line));
        }
        diagnostic
    };
    let small_diagnostic = build(16);
    let big_diagnostic = build(16 * SIZE_RATIO as u32);
    let small = median_nanos(|| {
        std::hint::black_box(human(&small_diagnostic, &catalog));
    });
    let big = median_nanos(|| {
        std::hint::black_box(human(&big_diagnostic, &catalog));
    });
    assert!(
        big < small * LINEAR_CEILING,
        "human scaled {small}ns -> {big}ns over x{SIZE_RATIO} output; \
         the linear shape allows at most x{LINEAR_CEILING}"
    );
}

#[test]
fn human_is_linear_across_sources() {
    // Every label in its own source — the cross-source worst case,
    // which a single-source fixture cannot measure.
    let build = |count: u32| {
        let mut catalog = SourceSet::new();
        let mut diagnostic: Option<Diagnostic> = None;
        for ordinal in 0..count {
            let file = catalog
                .add(format!("s{ordinal}.lp"), LINE.to_owned())
                .expect("test text admits");
            let label = Label {
                location: Location {
                    source: file,
                    span: Span::new(ByteOffset::new(0), ByteOffset::new(4))
                        .expect("ordered endpoints"),
                },
                message: Some("here".to_owned()),
            };
            diagnostic = Some(match diagnostic {
                None => Diagnostic::new(
                    DiagnosticId::new("syntax", "unexpected-token"),
                    Severity::Error,
                    "shape".to_owned(),
                    label,
                )
                .expect("non-empty headline"),
                Some(diagnostic) => diagnostic.with_secondary(label),
            });
        }
        (catalog, diagnostic.expect("count is never zero"))
    };
    let (small_catalog, small_diagnostic) = build(16);
    let (big_catalog, big_diagnostic) = build(16 * SIZE_RATIO as u32);
    let small = median_nanos(|| {
        std::hint::black_box(human(&small_diagnostic, &small_catalog));
    });
    let big = median_nanos(|| {
        std::hint::black_box(human(&big_diagnostic, &big_catalog));
    });
    assert!(
        big < small * LINEAR_CEILING,
        "human scaled {small}ns -> {big}ns over x{SIZE_RATIO} \
         sources; the linear shape allows at most x{LINEAR_CEILING}"
    );
}
