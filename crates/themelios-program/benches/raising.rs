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
    /// The statements the source writes — the occurrences the raise keeps.
    statements: usize,
    /// The statements the program keeps after the set merges content-equal ones.
    merged: usize,
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
        merged: size + 2,
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
        merged: 2 * size - 1,
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
        merged: size,
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
        merged: size,
        source,
    }
}

/// One choice of `size` boolean elements, every one a kept repeat (§4.4).
fn repeated_elements(size: usize) -> Fixture {
    let mut source = String::from("1 { ");
    for i in 0..size {
        if i > 0 {
            source.push_str("; ");
        }
        source.push_str("#true");
    }
    source.push_str(" } 1.\n");
    Fixture {
        name: "repeated_elements",
        size,
        statements: 1,
        merged: 1,
        source,
    }
}

/// `size` copies of one fact: one statement kept, every origin unioned into it.
fn repeated_statements(size: usize) -> Fixture {
    Fixture {
        name: "repeated_statements",
        size,
        statements: size,
        merged: 1,
        source: "a. ".repeat(size),
    }
}

/// A part of `size` formals over `size` distinct facts.
fn wide_part(size: usize) -> Fixture {
    let formals: Vec<String> = (1..=size).map(|i| format!("f{i}")).collect();
    let mut source = format!("#program p({}).\n", formals.join(", "));
    for i in 1..=size {
        write!(source, "a{i}. ").expect("writing to a String");
    }
    Fixture {
        name: "wide_part",
        size,
        statements: size,
        merged: size,
        source,
    }
}

/// The rule `p :- q, …, q.` with `size` copies of its body literal: one statement, one body
/// element.
fn repeated_body_literals(size: usize) -> Fixture {
    Fixture {
        name: "repeated_body_literals",
        size,
        statements: 1,
        merged: 1,
        source: format!("p :- {}.", vec!["q"; size].join(", ")),
    }
}

/// `size` distinct constants in `base`: every one a global definition the raise checks.
fn distinct_constants(size: usize) -> Fixture {
    let mut source = String::new();
    for i in 0..size {
        write!(source, "#const c{i} = {i}. ").expect("writing to a String");
    }
    Fixture {
        name: "distinct_constants",
        size,
        statements: size,
        merged: size,
        source,
    }
}

/// One choice of `size` copies of the atom element `a`: every repeat merged into one entry
/// by the counted constructor, each origin unioned into it (§4.4).
fn repeated_atom_elements(size: usize) -> Fixture {
    Fixture {
        name: "repeated_atom_elements",
        size,
        statements: 1,
        merged: 1,
        source: format!("{{ {} }}.", vec!["a"; size].join("; ")),
    }
}

/// Refuse a malformed fixture before measuring any operation on it.
fn checked_parse(fixture: &Fixture) -> Parse<ast::Program> {
    let source = Source::new(SourceId::new(0), fixture.source.clone()).expect("source admits");
    let parsed = parse(&source, Dialect::Clingo);
    assert!(parsed.diagnostics().is_empty(), "fixture parses cleanly");
    let direct = raise(&parsed);
    assert!(direct.diagnostics().is_empty(), "fixture raises cleanly");
    assert_eq!(direct.program().statements().count(), fixture.merged);
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
        repeated_elements(1_000),
        repeated_elements(16_000),
        repeated_statements(1_000),
        repeated_statements(16_000),
        wide_part(1_000),
        wide_part(16_000),
        repeated_body_literals(1_000),
        repeated_body_literals(16_000),
        distinct_constants(1_000),
        distinct_constants(16_000),
        repeated_atom_elements(1_000),
        repeated_atom_elements(16_000),
    ] {
        measure(&mut criterion, &fixture);
    }
    criterion.final_summary();
}
