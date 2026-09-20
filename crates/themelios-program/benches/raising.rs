//! Raising from an already parsed program (docs/design/program.md §8).
//!
//! The input source and parse are prepared once, outside every timed operation.
//! Each operation explicitly drops its output inside the timer. Collection via
//! `into_raised` prepares a fresh occurrence owner outside the timer for each
//! iteration, without timing a clone or retaining a batch of large owners.
//! Its timer therefore also includes destruction of the consumed owner.
//!
//! These are three public operation costs, not an additive phase decomposition:
//! `raise` and `raise_occurrences` place canonicalization differently. They do
//! not measure parsing, solving, allocation counts, or peak memory.

use std::fmt::Write;
use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput};
use themelios_base::source::{Source, SourceId};
use themelios_program::raise::{raise, raise_occurrences};
use themelios_syntax::ast;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::{Parse, parse};

struct Fixture {
    name: &'static str,
    size: usize,
    statements: usize,
    source: String,
}

/// A fact per edge and two rules, matching the plain reachability-chain shape.
fn chain(size: usize) -> Fixture {
    let mut source = String::new();
    for i in 0..size {
        if i > 0 {
            source.push(' ');
        }
        write!(source, "e({i},{}).", i + 1).expect("writing to a String");
    }
    source.push_str("\nr(0).\nr(Y) :- r(X), e(X,Y).\n");
    Fixture {
        name: "chain",
        size,
        statements: size + 2,
        source,
    }
}

/// Many nullary names, with two positive body literals in each producer rule.
fn producer_chain(size: usize) -> Fixture {
    let mut source = String::new();
    for i in 1..=size {
        if i > 1 {
            source.push(' ');
        }
        write!(source, "p{i}.").expect("writing to a String");
    }
    source.push('\n');
    for i in 1..size {
        writeln!(source, "q{i} :- p{i}, p{}.", i + 1).expect("writing to a String");
    }
    Fixture {
        name: "producer_chain",
        size,
        statements: 2 * size - 1,
        source,
    }
}

/// Ground leaves without bodies, with distinct values so every fact survives.
fn facts(size: usize) -> Fixture {
    let mut source = String::new();
    for i in 0..size {
        writeln!(source, "p({i},c).").expect("writing to a String");
    }
    Fixture {
        name: "facts",
        size,
        statements: size,
        source,
    }
}

/// Ground constructors, tuples, an unevaluated operator and a term pool.
fn structured(size: usize) -> Fixture {
    let mut source = String::new();
    for i in 0..size {
        writeln!(source, "p({i},f({i}),({i},c),X+1,(a;b)) :- q(X).").expect("writing to a String");
    }
    Fixture {
        name: "structured",
        size,
        statements: size,
        source,
    }
}

/// Refuse a malformed fixture before measuring any operation on it.
fn checked_parse(fixture: &Fixture) -> Parse<ast::Program> {
    let source = Source::new(SourceId::new(0), fixture.source.clone()).expect("source admits");
    let parsed = parse(&source, Dialect::Clingo);
    assert!(parsed.diagnostics().is_empty(), "fixture parses cleanly");
    let direct = raise(&parsed);
    assert!(direct.diagnostics().is_empty(), "fixture raises cleanly");
    assert_eq!(direct.program().statements().count(), fixture.statements);
    let occurrences = raise_occurrences(&parsed);
    assert!(occurrences.diagnostics().is_empty());
    assert_eq!(occurrences.occurrences().len(), fixture.statements);
    let collected = occurrences.into_raised();
    assert_eq!(collected.program(), direct.program());
    assert_eq!(collected.diagnostics(), direct.diagnostics());
    parsed
}

fn measure(criterion: &mut Criterion, fixture: &Fixture) {
    let parsed = checked_parse(fixture);
    let mut group = criterion.benchmark_group(format!("raising/{}", fixture.name));
    group.throughput(Throughput::Bytes(
        u64::try_from(fixture.source.len()).expect("fixture size fits u64"),
    ));
    group.bench_function(BenchmarkId::new("raise_drop", fixture.size), |b| {
        b.iter(|| drop(black_box(raise(black_box(&parsed)))));
    });
    group.bench_function(BenchmarkId::new("occurrences_drop", fixture.size), |b| {
        b.iter(|| drop(black_box(raise_occurrences(black_box(&parsed)))));
    });
    group.bench_function(BenchmarkId::new("collect_drop", fixture.size), |b| {
        b.iter_batched(
            || raise_occurrences(black_box(&parsed)),
            |occurrences| drop(black_box(occurrences.into_raised())),
            BatchSize::PerIteration,
        );
    });
    group.finish();
}

fn main() {
    let mut criterion = Criterion::default().configure_from_args();
    for fixture in [
        chain(1_000),
        chain(2_000),
        chain(4_000),
        producer_chain(700),
        facts(2_000),
        structured(1_000),
    ] {
        measure(&mut criterion, &fixture);
    }
    criterion.final_summary();
}
