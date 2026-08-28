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
use themelios_program::program::{Atom, Program, Rule, Statement};
use themelios_program::provenance::WithProvenance;
use themelios_program::symbol::Name;

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
