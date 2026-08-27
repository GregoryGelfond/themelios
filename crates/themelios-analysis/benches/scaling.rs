//! Scaling shapes for the analysis (docs/design/analysis.md §10), measured out of
//! band as criterion benchmarks: the strongly-connected-components decomposition (§4)
//! linear in the graph, the ASP-Core-2 safety binding fixpoint (§5) linear in the rule,
//! the head-cycle-free classification (§6.2) linear in a disjunctive head sharing a
//! positive cycle, and the whole analysis (`Analysis::of`, §3) linear in
//! `program + edges` — the four facets sharing one graph and scan. The test suite
//! asserts the shapes hold (the in-suite scaling tripwires, `tests/scc_laws.rs`,
//! `tests/safe_laws.rs`, `tests/classify_laws.rs`), while these benchmarks measure the
//! absolute numbers across doubling sizes, so a super-linear regression shows as a
//! curve (docs/specification.md §10.2). Run with `cargo bench`.

use criterion::{BenchmarkId, Criterion};

use themelios_analysis::analysis::Analysis;
use themelios_analysis::classify::Classes;
use themelios_analysis::depend::DependencyGraph;
use themelios_analysis::safe::Safety;
use themelios_program::program::{
    Atom, BodyElement, Comparison, Condition, DefaultNegation, Disjunction, DisjunctionElement,
    Head, Literal, LiteralInner, Program, Relation, Rule, Statement,
};
use themelios_program::provenance::WithProvenance;
use themelios_program::symbol::{Name, Symbol, VarName};
use themelios_program::term::{Term, Variable};

const SIZES: [usize; 3] = [1_000, 4_000, 16_000];

fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}

fn variable(i: usize) -> Term {
    Term::Variable(Variable::Named(
        VarName::new(format!("X{i}")).expect("a valid variable"),
    ))
}

fn edge(head: usize, body: usize) -> WithProvenance<Statement> {
    WithProvenance::constructed(Statement::Rule(Rule::new(
        Atom::constant(name(&format!("p{head}"))),
        Atom::constant(name(&format!("p{body}"))),
    )))
}

/// A chain `p0 :- p1. … p_{n-1} :- p_n.` — a deep acyclic dependency graph.
fn chain(n: usize) -> Program {
    Program::of((0..n).map(|i| edge(i, i + 1)))
}

/// A cycle `p0 :- p1. … p_{n-1} :- p0.` — one strongly-connected component of `n`.
fn cycle(n: usize) -> Program {
    Program::of((0..n).map(|i| edge(i, (i + 1) % n)))
}

/// One rule `p(X_n) :- X0 = 0, X1 = X0 + 1, …, X_n = X_{n-1} + 1.` — an assignment
/// chain of `n` variables, each bound after the previous.
fn assignment_chain(n: usize) -> Program {
    let assign = |lhs: usize, rhs: Term| {
        BodyElement::Literal(Literal {
            negation: DefaultNegation::None,
            inner: LiteralInner::Comparison(WithProvenance::constructed(Comparison::new(
                variable(lhs),
                Relation::Eq,
                rhs,
            ))),
        })
    };
    let mut body = vec![assign(0, Term::Symbolic(Symbol::Number(0)))];
    for i in 1..=n {
        body.push(assign(
            i,
            variable(i - 1) + Term::Symbolic(Symbol::Number(1)),
        ));
    }
    let head = Atom::new(name("p"), [variable(n)]);
    Program::of([WithProvenance::constructed(Statement::Rule(Rule::new(
        head, body,
    )))])
}

/// The decomposition (`DependencyGraph::of`, the walk and the iterative Tarjan) over a
/// chain and a cycle at doubling sizes — linear in the graph (§4).
fn decompose(c: &mut Criterion) {
    let mut group = c.benchmark_group("scc_decompose");
    for n in SIZES {
        let chain = chain(n);
        group.bench_with_input(BenchmarkId::new("chain", n), &chain, |b, program| {
            b.iter(|| DependencyGraph::of(program));
        });
        let cycle = cycle(n);
        group.bench_with_input(BenchmarkId::new("cycle", n), &cycle, |b, program| {
            b.iter(|| DependencyGraph::of(program));
        });
    }
    group.finish();
}

/// The safety binding fixpoint (`Safety::of`) over an assignment chain at doubling
/// sizes — linear in the rule (§5), where a re-scan would be quadratic.
fn binding_fixpoint(c: &mut Criterion) {
    let mut group = c.benchmark_group("safety_fixpoint");
    for n in SIZES {
        let program = assignment_chain(n);
        group.bench_with_input(BenchmarkId::new("chain", n), &program, |b, program| {
            b.iter(|| Safety::of(program));
        });
    }
    group.finish();
}

/// A large disjunctive head sharing a positive cycle: `(p0 ; … ; p_{n-1}) :- q.` plus the
/// cycle `p0 :- p1. … p_{n-1} :- p0.` — every head atom in one positive component, the
/// shape head-cycle-freeness scans (§6.2).
fn disjunctive_over_cycle(n: usize) -> Program {
    let constant = |i: usize| Atom::constant(name(&format!("p{i}")));
    let elements =
        (0..n).map(|i| DisjunctionElement::new(Literal::from(constant(i)), Condition::empty()));
    let head = Head::Disjunction(Disjunction::new(elements)).when(Atom::constant(name("q")));
    let mut statements = vec![WithProvenance::constructed(Statement::Rule(head))];
    for i in 0..n {
        statements.push(WithProvenance::constructed(Statement::Rule(Rule::new(
            constant(i),
            constant((i + 1) % n),
        ))));
    }
    Program::of(statements)
}

/// The classification (`Classes::of`) over a large disjunctive head sharing a positive
/// cycle at doubling sizes — the head-cycle-free scan linear in the head (§6.2), where a
/// pairwise check over the head would be quadratic.
fn classify_head_cycles(c: &mut Criterion) {
    let mut group = c.benchmark_group("classify_head_cycles");
    for n in SIZES {
        let program = disjunctive_over_cycle(n);
        group.bench_with_input(
            BenchmarkId::new("disjunction_over_cycle", n),
            &program,
            |b, program| {
                b.iter(|| Classes::of(program));
            },
        );
    }
    group.finish();
}

/// The whole analysis (`Analysis::of`, the one shared-graph pass) over a chain and a
/// cycle at doubling sizes — linear in `program + edges` (§3), the construct scan and
/// the dependency graph built once and shared across the four facets.
fn analyze(c: &mut Criterion) {
    let mut group = c.benchmark_group("analysis_of");
    for n in SIZES {
        let chain = chain(n);
        group.bench_with_input(BenchmarkId::new("chain", n), &chain, |b, program| {
            b.iter(|| Analysis::of(program));
        });
        let cycle = cycle(n);
        group.bench_with_input(BenchmarkId::new("cycle", n), &cycle, |b, program| {
            b.iter(|| Analysis::of(program));
        });
    }
    group.finish();
}

// The harness, written out rather than generated by `criterion_group!` and
// `criterion_main!`: the macros expand to exactly this, and the generated group is a
// public function without documentation, which the workspace's denied `missing_docs`
// refuses. Same behavior, one documented item.

/// The scaling group: every bench above, under criterion's default configuration as
/// adjusted by the command line.
pub fn scaling() {
    let mut criterion: Criterion = Criterion::default().configure_from_args();
    decompose(&mut criterion);
    binding_fixpoint(&mut criterion);
    classify_head_cycles(&mut criterion);
    analyze(&mut criterion);
}

fn main() {
    scaling();
    Criterion::default().configure_from_args().final_summary();
}
