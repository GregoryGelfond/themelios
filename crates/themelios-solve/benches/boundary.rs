//! The solve tier's boundary (docs/design/solve.md §10.2), measured out of band
//! as criterion benchmarks: the absolute curves whose near-linear shapes the
//! in-suite tripwires (tests/scaling_shape.rs) assert. Each input is prepared
//! outside the timed operation, and each operation drops its output inside the
//! timer. A human reads the real curves here when tuning; the checks hold only
//! the machine-independent shapes. Run with `cargo bench`.

use std::fmt::Write;
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion};
use themelios_base::source::{Source, SourceId};
use themelios_solve::bridge::Admitted;
use themelios_syntax::{Dialect, parse};

/// The sizes each curve is read over.
const SIZES: [usize; 2] = [1_000, 16_000];

/// `size` distinct facts.
fn distinct_facts(size: usize) -> String {
    let mut text = String::new();
    for i in 0..size {
        write!(text, "p({i}). ").expect("writing to a string does not fail");
    }
    text
}

/// Door A's admission of a parse of distinct facts, and of repeated ones.
fn admission(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("admission");
    for size in SIZES {
        for (shape, text) in [
            ("distinct", distinct_facts(size)),
            ("repeated", "a. ".repeat(size)),
        ] {
            let source = Source::new(SourceId::new(0), text).expect("the source admits");
            let parsed = parse(&source, Dialect::Clingo);
            group.bench_with_input(BenchmarkId::new(shape, size), &parsed, |bencher, parsed| {
                bencher.iter(|| drop(black_box(Admitted::of(black_box(parsed)))));
            });
        }
    }
    group.finish();
}

fn main() {
    let mut criterion = Criterion::default().configure_from_args();
    admission(&mut criterion);
    criterion.final_summary();
}
