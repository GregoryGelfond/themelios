//! Shape assertions for the checks (docs/design/analysis.md §10): complexity shape
//! only, held by the median over five interleaved wall-clock ratios with tolerances
//! wide enough for any machine the checks run on — the whole analysis (`Analysis::of`)
//! linear in `program + edges` (§3), and the strongly-connected-components
//! decomposition linear in the graph (§4, the iterative Tarjan). What they prove: the
//! claimed class — a quadratic `Analysis::of`, a quadratic decomposition. What they
//! cannot: absolute speed, which is machine-dependent and lives in the out-of-band
//! benches (benches/scaling.rs, spec §10.2).
//!
//! Distinct from the absolute-size tripwires the facet suites already carry
//! (tests/scc_laws.rs, tests/safe_laws.rs, tests/classify_laws.rs): those run a fixed
//! large program and assert it *finishes* — the guard against non-termination or a
//! catastrophically bad bound. These assert the *growth ratio* across two sizes — the
//! guard against a complexity regression (linear → quadratic) a run that merely
//! finishes would not catch. The two are complementary. The safety fixpoint (§5) and
//! the head-cycle-free scan (§6.2) have their absolute curves in the benches and their
//! correctness and termination in their facet suites; `Analysis::of` runs them all, so
//! a quadratic in any facet a growing chain exercises would surface in the whole-pass
//! ratio here too.
//!
//! Each ratio is the median over five runs that time the small case and the large case
//! back-to-back, not the ratio of two separately-median'd batches: a load transient
//! during a run inflates both of that run's halves and cancels in its ratio, so no
//! transient landing on the large measurement alone can push the ratio past its
//! ceiling.

use std::time::Instant;

use themelios_analysis::analysis::Analysis;
use themelios_analysis::depend::DependencyGraph;
use themelios_program::program::{
    Atom, BodyElement, Comparison, Condition, DefaultNegation, Disjunction, DisjunctionElement,
    Head, Literal, LiteralInner, Program, Relation, Rule, Statement,
};
use themelios_program::provenance::WithProvenance;
use themelios_program::symbol::{Name, VarName};
use themelios_program::term::{Term, Variable};

/// The data-size ratio between the small and large cases.
const SIZE_RATIO: usize = 16;
/// A linear claim at SIZE_RATIO may cost at most this factor: fourfold noise headroom
/// above linear (x16) and fourfold separation below quadratic (x256).
const LINEAR_CEILING: u128 = SIZE_RATIO as u128 * 4;
/// Interleaved runs per measurement; the median of their ratios is taken.
const SAMPLES: usize = 5;
/// Ratios are scaled by this factor so the median arithmetic stays in integers; a
/// ceiling `C` is the scaled bound `C * RATIO_SCALE`.
const RATIO_SCALE: u128 = 1000;
/// The base program size; the large case is SIZE_RATIO larger.
const BASE: usize = 1_000;

/// One elapsed measurement of `work`, in nanoseconds — floored to 1 so a
/// sub-nanosecond reading can still divide.
fn time_once(mut work: impl FnMut()) -> u128 {
    let start = Instant::now();
    work();
    start.elapsed().as_nanos().max(1)
}

/// The median over SAMPLES interleaved runs of `big`'s cost over `small`'s, scaled by
/// RATIO_SCALE. Each run evaluates `small` then `big` back-to-back; the two closures
/// return that run's cost figure (elapsed nanos).
fn median_ratio(mut small: impl FnMut() -> u128, mut big: impl FnMut() -> u128) -> u128 {
    let mut ratios = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let s = small().max(1);
        let b = big();
        ratios.push(b * RATIO_SCALE / s);
    }
    ratios.sort_unstable();
    ratios[SAMPLES / 2]
}

fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}

/// The rule `p<head> :- p<body>.` — one dependency edge.
fn edge(head: usize, body: usize) -> WithProvenance<Statement> {
    WithProvenance::constructed(Statement::Rule(Rule::new(
        Atom::constant(name(&format!("p{head}"))),
        Atom::constant(name(&format!("p{body}"))),
    )))
}

/// A chain `p0 :- p1. … p_{n-1} :- p_n.` — `n` edges, `n + 1` predicates, an acyclic
/// graph of `n + 1` singleton components.
fn chain(n: usize) -> Program {
    Program::of((0..n).map(|i| edge(i, i + 1)))
}

/// A cycle `p0 :- p1. … p_{n-1} :- p0.` — `n` edges in one strongly-connected component
/// of size `n`.
fn cycle(n: usize) -> Program {
    Program::of((0..n).map(|i| edge(i, (i + 1) % n)))
}

/// `n` self-loops `p0 :- p0. … p_{n-1} :- p_{n-1}.` — `n` predicates, `n` edges, and `n`
/// recursive components, each a single node with a positive self-edge. The shape that
/// stresses grounding finiteness (§5): it reads the rules deriving each recursive
/// component, so a per-component rescan of the whole program is Θ(n²) here while the
/// indexed pass is linear.
fn self_loops(n: usize) -> Program {
    Program::of((0..n).map(|i| edge(i, i)))
}

/// One recursive rule `p(g(X0)) :- p(Xn), X0 = X1, …, X_{n-1} = Xn.` — an `n`-link equality
/// chain the finiteness `=`-alias closure must traverse (`X0`, deepened under `g` in the
/// head, is aliased to the recursive `Xn`). A re-scan closure is Θ(n²) here; the worklist
/// closure is linear. `self_loops` cannot catch this — it is equality-free.
fn equality_chain(n: usize) -> Program {
    let var = |index: usize| {
        Term::Variable(Variable::Named(
            VarName::new(format!("X{index}")).expect("a valid variable"),
        ))
    };
    let head = Atom::new(
        name("p"),
        [Term::Function {
            name: name("g"),
            arguments: vec![var(0)],
        }],
    );
    let mut body: Vec<BodyElement> = vec![BodyElement::from(Atom::new(name("p"), [var(n)]))];
    for i in 0..n {
        body.push(BodyElement::Literal(Literal {
            negation: DefaultNegation::None,
            inner: LiteralInner::Comparison(WithProvenance::constructed(Comparison::new(
                var(i),
                Relation::Eq,
                var(i + 1),
            ))),
        }));
    }
    Program::of([WithProvenance::constructed(Statement::Rule(Rule::new(
        head, body,
    )))])
}

/// One rule with an `n`-way disjunctive head over `n` self-loops:
/// `p0 ; … ; p_{n-1} :- base.` plus `p0 :- p0. … p_{n-1} :- p_{n-1}.` — the disjunctive
/// rule derives into all `n` recursive components, so a finiteness pass that re-scans the
/// whole head once per component is Θ(n²) here; charging each rule O(|rule|) is linear.
/// Neither `self_loops` (single-head) nor `equality_chain` (one rule, one component)
/// exercises the multi-head fan-out.
fn wide_head_over_self_loops(n: usize) -> Program {
    let head = Head::Disjunction(Disjunction::new((0..n).map(|i| {
        DisjunctionElement::new(
            Literal::from(Atom::constant(name(&format!("p{i}")))),
            Condition::empty(),
        )
    })));
    let wide = Statement::Rule(head.when(Atom::constant(name("base"))));
    let loops = (0..n).map(|i| {
        Statement::Rule(Rule::new(
            Atom::constant(name(&format!("p{i}"))),
            Atom::constant(name(&format!("p{i}"))),
        ))
    });
    Program::of(
        std::iter::once(wide)
            .chain(loops)
            .map(WithProvenance::constructed),
    )
}

/// A cycle of `n` predicates each carrying a variable — `p0(X) :- p1(X). … p_{n-1}(X) :- p0(X).`
/// — one recursive component of size `n`, with no term growth (so the finiteness pass walks
/// every rule rather than stopping at a witness). A pass that, for each rule deriving into the
/// component, reads the component's whole member list is Θ(n²) here; charging each rule
/// `O(|rule|)` — the carried variables grouped by component once off the body, not re-derived
/// from the component's members per rule — is linear. Neither `self_loops` (`n` singleton
/// components) nor `wide_head_over_self_loops` (one wide head, singleton components) exercises
/// this shape: one big component with many rules deriving into it.
fn giant_recursive_component(n: usize) -> Program {
    let var = Term::Variable(Variable::Named(
        VarName::new("X").expect("a valid variable"),
    ));
    Program::of((0..n).map(|i| {
        WithProvenance::constructed(Statement::Rule(Rule::new(
            Atom::new(name(&format!("p{i}")), [var.clone()]),
            Atom::new(name(&format!("p{}", (i + 1) % n)), [var.clone()]),
        )))
    }))
}

#[test]
fn analysis_of_is_linear_in_the_program() {
    // The whole analysis is one shared-graph pass (§3): the construct scan and the
    // dependency graph built once and read by every facet. Over a chain (the general
    // program + edges shape) it is linear; a quadratic in the shared pass, or in any
    // facet the chain exercises, would show here.
    let small = chain(BASE);
    let big = chain(BASE * SIZE_RATIO);
    let ratio = median_ratio(
        || {
            time_once(|| {
                std::hint::black_box(Analysis::of(&small));
            })
        },
        || {
            time_once(|| {
                std::hint::black_box(Analysis::of(&big));
            })
        },
    );
    let approx = ratio / RATIO_SCALE;
    assert!(
        ratio < LINEAR_CEILING * RATIO_SCALE,
        "Analysis::of's median ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over x{SIZE_RATIO} program; the linear shape allows at most x{LINEAR_CEILING}"
    );
}

#[test]
fn the_scc_decomposition_is_linear_in_the_graph() {
    // A cycle is one strongly-connected component of size `n` — the shape that most
    // stresses the iterative Tarjan's component assembly (§4). Building the graph and
    // decomposing it is linear in the graph; a quadratic decomposition (a re-scan per
    // vertex) would show here as a ratio near SIZE_RATIO², well past the ceiling.
    let small = cycle(BASE);
    let big = cycle(BASE * SIZE_RATIO);
    let ratio = median_ratio(
        || {
            time_once(|| {
                std::hint::black_box(DependencyGraph::of(&small));
            })
        },
        || {
            time_once(|| {
                std::hint::black_box(DependencyGraph::of(&big));
            })
        },
    );
    let approx = ratio / RATIO_SCALE;
    assert!(
        ratio < LINEAR_CEILING * RATIO_SCALE,
        "the decomposition's median ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over x{SIZE_RATIO} graph; the linear shape allows at most x{LINEAR_CEILING}"
    );
}

#[test]
fn finiteness_is_linear_in_many_recursive_components() {
    // Grounding finiteness (§5) reads the rules deriving each recursive component. Over
    // `n` self-loops — `n` recursive components — a rescan of the whole program per
    // component is Θ(n²); the indexed pass (each rule read once, off a head-signature
    // index) is linear. `chain` and `cycle` cannot catch this: a chain has no recursive
    // component and a cycle has exactly one, so neither exercises the many-components
    // shape the quadratic needs — the shape a shape-blind tripwire lets through.
    let small = self_loops(BASE);
    let big = self_loops(BASE * SIZE_RATIO);
    let ratio = median_ratio(
        || {
            time_once(|| {
                std::hint::black_box(Analysis::of(&small));
            })
        },
        || {
            time_once(|| {
                std::hint::black_box(Analysis::of(&big));
            })
        },
    );
    let approx = ratio / RATIO_SCALE;
    assert!(
        ratio < LINEAR_CEILING * RATIO_SCALE,
        "finiteness's median ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over x{SIZE_RATIO} recursive components; the linear shape allows at most x{LINEAR_CEILING}"
    );
}

#[test]
fn finiteness_is_linear_in_an_equality_chain() {
    // The finiteness `=`-alias closure (§5) traverses a rule's equality chain. Over an
    // `n`-link chain a re-scan closure is Θ(n²) (up to `n` passes over `n` groups); the
    // worklist closure, absorbing each group once, is linear. `self_loops` above guards
    // the component-loop half of the finiteness pass; this guards the equality-closure half.
    let small = equality_chain(BASE);
    let big = equality_chain(BASE * SIZE_RATIO);
    let ratio = median_ratio(
        || {
            time_once(|| {
                std::hint::black_box(Analysis::of(&small));
            })
        },
        || {
            time_once(|| {
                std::hint::black_box(Analysis::of(&big));
            })
        },
    );
    let approx = ratio / RATIO_SCALE;
    assert!(
        ratio < LINEAR_CEILING * RATIO_SCALE,
        "finiteness's median ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over x{SIZE_RATIO} equality chain; the linear shape allows at most x{LINEAR_CEILING}"
    );
}

#[test]
fn finiteness_is_linear_in_a_wide_head_over_many_components() {
    // A single rule whose `n`-way disjunctive head derives into `n` recursive components
    // (§5): a finiteness pass that re-analyzes that rule — rebuilding its whole head — once
    // per component is Θ(n²); charging each rule O(|rule|) total, examining each head atom
    // once against its own component, is linear. This guards the multi-head fan-out that
    // `self_loops` (single-head) and `equality_chain` (one component) cannot.
    let small = wide_head_over_self_loops(BASE);
    let big = wide_head_over_self_loops(BASE * SIZE_RATIO);
    let ratio = median_ratio(
        || {
            time_once(|| {
                std::hint::black_box(Analysis::of(&small));
            })
        },
        || {
            time_once(|| {
                std::hint::black_box(Analysis::of(&big));
            })
        },
    );
    let approx = ratio / RATIO_SCALE;
    assert!(
        ratio < LINEAR_CEILING * RATIO_SCALE,
        "finiteness's median ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over x{SIZE_RATIO} wide head; the linear shape allows at most x{LINEAR_CEILING}"
    );
}

#[test]
fn finiteness_is_linear_in_a_giant_recursive_component() {
    // One recursive component of size `n` with `n` rules deriving into it (§5): a finiteness
    // pass that reads the component's whole member list once per rule deriving into it is
    // Θ(n²); grouping each rule's carried variables by component once off the body, so a head
    // atom's component is a lookup rather than a member scan, is linear. `self_loops` (singleton
    // components) and `wide_head_over_self_loops` (one wide head over singletons) both leave this
    // shape — many rules over one big component — uncovered.
    let small = giant_recursive_component(BASE);
    let big = giant_recursive_component(BASE * SIZE_RATIO);
    let ratio = median_ratio(
        || {
            time_once(|| {
                std::hint::black_box(Analysis::of(&small));
            })
        },
        || {
            time_once(|| {
                std::hint::black_box(Analysis::of(&big));
            })
        },
    );
    let approx = ratio / RATIO_SCALE;
    assert!(
        ratio < LINEAR_CEILING * RATIO_SCALE,
        "finiteness's median ratio was ~x{approx} ({ratio}/{RATIO_SCALE}) over x{SIZE_RATIO} component members; the linear shape allows at most x{LINEAR_CEILING}"
    );
}
