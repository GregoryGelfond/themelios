//! The epistemic reading's matching scan (docs/design/query.md §3.1; docs/design/solve.md §13.3),
//! measured out of band as a criterion benchmark: the absolute curve whose near-linear shape the
//! in-suite scaling tripwire asserts. `bindings` finds a pattern's instances by the program
//! tier's signature-range scan and most general unifier; this measures it over a member holding
//! one ground symbol of growing depth — a per-level re-walk would be quadratic. A human reads the
//! real curve and its constants here when tuning; the checks hold only the machine-independent
//! shape. Run with `cargo bench`.

use criterion::{BenchmarkId, Criterion};

use themelios_program::{AnswerSet, Atom, Name, Program, Sign, Symbol, Term};
use themelios_query::{AgentReading, BindingPattern, Snapshot};
use themelios_solve::agent::{Agent, Scenario};
use themelios_solve::bridge::Door;
use themelios_solve::contract::{Backend, Capabilities, Fault, SolveRequest};
use themelios_solve::outcome::{Conclusion, Model, Run, ShowRule, Solved};

/// The symbol depths the curve is read over.
const DEPTHS: [usize; 4] = [1_000, 2_000, 4_000, 8_000];

/// A search that yields its one member, then closes the space.
struct One {
    member: Option<AnswerSet>,
}

impl Run for One {
    fn next_model(&mut self) -> Option<Result<Model, Fault>> {
        self.member.take().map(|member| Ok(Model::of(member)))
    }

    fn conclusion(&self) -> Option<Conclusion> {
        self.member.is_none().then_some(Conclusion::Exhausted)
    }
}

/// A backend whose world view is one member.
struct Holding {
    member: AnswerSet,
}

impl Backend for Holding {
    fn capabilities(&self) -> Capabilities {
        let mut capabilities = Capabilities::default();
        capabilities.enumeration = true;
        capabilities
    }

    fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
        let run = One {
            member: Some(self.member.clone()),
        };
        Ok(Solved::running(
            Box::new(run),
            Scenario::default(),
            ShowRule::default(),
        ))
    }

    fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
        Ok(())
    }
}

fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}

/// `f(f(… a …))`, `depth` applications deep.
fn nested(depth: usize) -> Symbol {
    let bottom = Symbol::function(name("a"), [], Sign::Positive);
    (0..depth).fold(bottom, |inner, _| {
        Symbol::function(name("f"), [inner], Sign::Positive)
    })
}

/// The snapshot of a world view whose one member is `p(symbol)`.
fn snapshot_holding(symbol: &Symbol) -> Snapshot {
    let atom = Symbol::function(name("p"), [symbol.clone()], Sign::Positive);
    let member: AnswerSet = [atom].into_iter().collect();
    let mut agent = Agent::new(Program::empty(), Holding { member });
    agent
        .snapshot()
        .expect("a one-member world view materialises")
}

/// The bindings of the ground pattern `p(symbol)` over a member holding it, for a symbol of each
/// depth.
fn deep_match(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("matching");
    for depth in DEPTHS {
        let symbol = nested(depth);
        let snapshot = snapshot_holding(&symbol);
        let pattern = BindingPattern::of(Atom::new(name("p"), [Term::from(symbol)]))
            .expect("a ground atom is a binding pattern");
        group.bench_with_input(
            BenchmarkId::new("deep ground symbol", depth),
            &pattern,
            |bencher, pattern| {
                bencher.iter(|| snapshot.bindings(pattern));
            },
        );
    }
    group.finish();
}

fn main() {
    let mut criterion = Criterion::default().configure_from_args();
    deep_match(&mut criterion);
    criterion.final_summary();
}
