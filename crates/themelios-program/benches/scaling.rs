//! Scaling shapes (docs/design/program.md §15, §16), measured out of band as criterion
//! benchmarks: the absolute curves whose *shapes* the checks assert in
//! tests/scaling_shape.rs. Each operation the tier's cost table names is measured over a
//! growing input — equality, clone, rendering, and traversal over a deep term (linear in
//! structure); `mgu` over a deep ground symbol against its non-ground twin (the near-linear
//! decision §11.1/§15 promises, the case a monolithic ground representation would make
//! quadratic) and over two non-ground terms; a match against an answer set via
//! `signature_range` (O(log n + k), §11.3); and part-wise access (O(log parts), §4.1). A
//! human reads the real curve and its constants here when tuning; the checks hold only the
//! machine-independent shape (spec §10.2). Run with `cargo bench`.

use std::collections::BTreeSet;

use criterion::{BenchmarkId, Criterion};

use themelios_base::source::{Source, SourceId};

use themelios_program::program::{Atom, PartKey, Program, Rule, Statement};
use themelios_program::provenance::WithProvenance;
use themelios_program::raise::raise;
use themelios_program::render::render;
use themelios_program::symbol::{Name, Sign, Symbol, VarName};
use themelios_program::term::{Term, Variable};
use themelios_program::unify::{mgu, signature_range};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}

fn variable(text: &str) -> Term {
    Term::Variable(Variable::Named(
        VarName::new(text).expect("a valid variable name"),
    ))
}

/// `f(f(… bottom …))` — `depth` `f`-applications over the bottom term.
fn nest(bottom: Term, depth: usize) -> Term {
    let mut term = bottom;
    for _ in 0..depth {
        term = Term::Function {
            name: name("f"),
            arguments: vec![term],
        };
    }
    term
}

/// A NON-ground deep term, `f(f(… X …))`, so a traversal descends the whole spine (a
/// ground nest collapses to a `Symbolic` leaf, §3.6).
fn deep_term(depth: usize) -> Term {
    nest(variable("X"), depth)
}

/// A one-rule program `q :- p(f(f(… X …))).` carrying a deep term, for the renderer.
fn deep_program(depth: usize) -> Program {
    let rule = Rule::new(
        Atom::constant(name("q")),
        Atom::new(name("p"), [deep_term(depth)]),
    );
    Program::of([WithProvenance::constructed(Statement::Rule(rule))])
}

const DEPTHS: [usize; 5] = [1_000, 2_000, 4_000, 8_000, 16_000];

fn mgu_scaling(c: &mut Criterion) {
    // A deep ground symbol against its non-ground twin — the case that was Θ(depth²) before the
    // ground side was decomposed into the unification graph (§11.1). Timings should grow linearly.
    let mut ground_vs_nested = c.benchmark_group("mgu/ground_vs_nested");
    for depth in DEPTHS {
        let ground = Atom::new(
            name("p"),
            [nest(
                Term::Function {
                    name: name("a"),
                    arguments: Vec::new(),
                },
                depth,
            )],
        );
        let nested = Atom::new(name("p"), [nest(variable("X"), depth)]);
        ground_vs_nested.bench_with_input(BenchmarkId::from_parameter(depth), &depth, |b, _| {
            b.iter(|| mgu(&ground, &nested));
        });
    }
    ground_vs_nested.finish();

    // Two non-ground terms of the same shape, a variable at each bottom — the term-vs-term path.
    let mut term_vs_term = c.benchmark_group("mgu/term_vs_term");
    for depth in DEPTHS {
        let left = Atom::new(name("p"), [nest(variable("X"), depth)]);
        let right = Atom::new(name("p"), [nest(variable("Y"), depth)]);
        term_vs_term.bench_with_input(BenchmarkId::from_parameter(depth), &depth, |b, _| {
            b.iter(|| mgu(&left, &right));
        });
    }
    term_vs_term.finish();
}

/// Equality, clone, rendering, and traversal over a deep term — each linear in the
/// structure (§13, §15). Equality times two equal terms, so the full walk runs (no
/// short-circuit); traversal counts every node `subterms` visits.
fn structural_scaling(c: &mut Criterion) {
    let mut clone_group = c.benchmark_group("clone/term_depth");
    for depth in DEPTHS {
        let term = deep_term(depth);
        clone_group.bench_with_input(BenchmarkId::from_parameter(depth), &depth, |b, _| {
            b.iter(|| term.clone());
        });
    }
    clone_group.finish();

    let mut equality_group = c.benchmark_group("equality/term_depth");
    for depth in DEPTHS {
        let left = deep_term(depth);
        let right = left.clone();
        equality_group.bench_with_input(BenchmarkId::from_parameter(depth), &depth, |b, _| {
            b.iter(|| left == right);
        });
    }
    equality_group.finish();

    let mut render_group = c.benchmark_group("render/term_depth");
    for depth in DEPTHS {
        let program = deep_program(depth);
        render_group.bench_with_input(BenchmarkId::from_parameter(depth), &depth, |b, _| {
            b.iter(|| render(&program, Dialect::Clingo).expect("renders"));
        });
    }
    render_group.finish();

    let mut subterms_group = c.benchmark_group("subterms/term_depth");
    for depth in DEPTHS {
        let term = deep_term(depth);
        subterms_group.bench_with_input(BenchmarkId::from_parameter(depth), &depth, |b, _| {
            b.iter(|| term.subterms().count());
        });
    }
    subterms_group.finish();
}

/// The fixed number of `p(_)` symbols the pattern `p(X)` matches, whatever the answer
/// set's size.
const TARGETS: usize = 8;

/// A ground unary symbol `pred(arg)`.
fn ground_symbol(pred: &str, arg: usize) -> Symbol {
    Symbol::Function {
        name: name(pred),
        arguments: vec![Symbol::Number(
            i32::try_from(arg).expect("the argument fits i32"),
        )],
        sign: Sign::Positive,
    }
}

/// An answer set: the fixed `p(0..TARGETS)` block the pattern matches, plus `fillers`
/// symbols of a predicate the pattern's range excludes.
fn answer_set(fillers: usize) -> BTreeSet<Symbol> {
    let mut set = BTreeSet::new();
    for i in 0..TARGETS {
        set.insert(ground_symbol("p", i));
    }
    for i in 0..fillers {
        set.insert(ground_symbol("q", i));
    }
    set
}

const ANSWER_SET_SIZES: [usize; 4] = [1_000, 4_000, 16_000, 64_000];

/// A match against an answer set via `signature_range` — O(log n + k) (§11.3): a range
/// scan, not a full scan. The target block stays fixed, so the cost grows with log n.
fn matching_scaling(c: &mut Criterion) {
    let pattern = Atom::new(name("p"), [variable("X")]);
    let mut group = c.benchmark_group("match/answer_set");
    for fillers in ANSWER_SET_SIZES {
        let set = answer_set(fillers);
        group.bench_with_input(BenchmarkId::from_parameter(fillers), &fillers, |b, _| {
            b.iter(|| set.range(signature_range(&pattern)).count());
        });
    }
    group.finish();
}

/// A program of `parts` parts, each opened by a `#program q<i>.` delimiter and holding
/// one fact — the raise's part door (§4.1, §8).
fn multi_part_program(parts: usize) -> Program {
    let mut text = String::with_capacity(parts * 16);
    for i in 0..parts {
        text.push_str("#program q");
        text.push_str(&i.to_string());
        text.push_str(".\na.\n");
    }
    let source = Source::new(SourceId::new(0), text).expect("the multi-part text admits");
    raise(&parse(&source, Dialect::Clingo)).into_program()
}

/// The key of the `q<i>` part.
fn q_key(i: usize) -> PartKey {
    PartKey {
        name: name(&format!("q{i}")),
        formals: Vec::new(),
    }
}

const PART_COUNTS: [usize; 4] = [64, 256, 1_024, 4_096];

/// Part-wise access — `Program::part` is a `BTreeMap` lookup, O(log parts) (§4.1, §15).
/// A fixed set of keys is looked up, so the cost grows with log parts.
fn part_access_scaling(c: &mut Criterion) {
    let keys: Vec<PartKey> = (0..64).map(q_key).collect();
    let mut group = c.benchmark_group("part_access");
    for parts in PART_COUNTS {
        let program = multi_part_program(parts);
        group.bench_with_input(BenchmarkId::from_parameter(parts), &parts, |b, _| {
            b.iter(|| {
                for key in &keys {
                    std::hint::black_box(program.part(key));
                }
            });
        });
    }
    group.finish();
}

// The harness, written out rather than generated by `criterion_group!` and `criterion_main!`:
// the macros expand to exactly this, and the generated group is a public function without
// documentation, which the workspace's denied `missing_docs` refuses. Same behavior, one
// documented item.

/// The scaling group: every bench above, under criterion's default configuration as adjusted by
/// the command line.
pub fn scaling() {
    let mut criterion: Criterion = Criterion::default().configure_from_args();
    mgu_scaling(&mut criterion);
    structural_scaling(&mut criterion);
    matching_scaling(&mut criterion);
    part_access_scaling(&mut criterion);
}

fn main() {
    scaling();
    Criterion::default().configure_from_args().final_summary();
}
