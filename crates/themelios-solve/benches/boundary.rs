//! The solve tier's boundary (docs/design/solve.md §5.1, §10.2), measured out of
//! band as criterion benchmarks: the absolute curves whose near-linear shapes the
//! in-suite tripwires (tests/scaling_shape.rs) assert. Each input is prepared
//! outside the timed operation, and each operation drops its output inside the
//! timer. A human reads the real curves here when tuning; the checks hold only
//! the machine-independent shapes. Run with `cargo bench`.

use std::fmt::Write;
use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion};
use themelios_base::source::{Source, SourceId};
use themelios_program::program::Show;
use themelios_program::symbol::{Name, Sign, Signature, Symbol};
use themelios_solve::agent::Scenario;
use themelios_solve::bridge::Admitted;
use themelios_solve::contract::Fault;
use themelios_solve::outcome::{Conclusion, Model, Run, ShowRule, Solved};
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

/// A run that yields one model, then reports its search closed.
struct OneModel {
    model: Option<Model>,
}

impl Run for OneModel {
    fn next_model(&mut self) -> Option<Result<Model, Fault>> {
        self.model.take().map(Ok)
    }

    fn conclusion(&self) -> Option<Conclusion> {
        self.model.is_none().then_some(Conclusion::Exhausted)
    }
}

/// The unary ground atom `name(i)`.
fn applied(name: &str, i: usize) -> Symbol {
    let i = i32::try_from(i).expect("a small index");
    Symbol::function(
        Name::new(name).expect("a valid identifier"),
        [Symbol::number(i)],
        Sign::Positive,
    )
}

/// A model of `count` atoms `p(i)` and `count` atoms `q(i)`, with `count`
/// terms `t(i)`.
fn wide_model(count: usize) -> Model {
    let atoms = (0..count)
        .flat_map(|i| [applied("p", i), applied("q", i)])
        .collect();
    Model::of(atoms).with_terms((0..count).map(|i| applied("t", i)))
}

/// The rule of `#show q/1.` — restricting, so the derivation filters every
/// atom.
fn showing_q() -> ShowRule {
    let name = Name::new("q").expect("a valid identifier");
    ShowRule::of([&Show::Signature(Signature {
        sign: Sign::Positive,
        name,
        arity: 1,
    })])
}

/// The core's derivation of one model's display under a restricting rule.
fn display(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("display");
    let rule = showing_q();
    for size in SIZES {
        let model = wide_model(size);
        group.bench_with_input(
            BenchmarkId::new("derive", size),
            &model,
            |bencher, model| {
                bencher.iter_batched(
                    || {
                        Solved::running(
                            Box::new(OneModel {
                                model: Some(model.clone()),
                            }),
                            Scenario::default(),
                            rule.clone(),
                        )
                    },
                    |mut solved| drop(black_box(solved.models().next())),
                    BatchSize::LargeInput,
                );
            },
        );
    }
    group.finish();
}

fn main() {
    let mut criterion = Criterion::default().configure_from_args();
    admission(&mut criterion);
    display(&mut criterion);
    criterion.final_summary();
}
