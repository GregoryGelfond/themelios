//! The knowledge base's rebuild (docs/design/solve.md §6.2, §13.3), measured out of band as a
//! criterion benchmark: the absolute curve whose linear shape the in-suite scaling tripwire
//! asserts. Each assertion rebuilds the agent's knowledge base from its ledger's live
//! statements, `Θ(program size)`; this measures one assertion into knowledge bases of growing
//! size, the agent built and dropped outside the timing. A human reads the real curve and its
//! constants here when tuning; the checks hold only the machine-independent shape. Run with
//! `cargo bench`.

use criterion::{BatchSize, BenchmarkId, Criterion};

use themelios_program::{Atom, Name, Program, Rule, Statement, Symbol, Term};
use themelios_solve::agent::Agent;
use themelios_solve::bridge::Door;
use themelios_solve::contract::{Backend, Capabilities, Fault, SolveRequest};
use themelios_solve::outcome::Solved;

/// The knowledge-base sizes the curve is read over.
const SIZES: [i32; 4] = [1_000, 2_000, 4_000, 8_000];

/// A backend the benchmark never asks: an assertion touches no engine.
struct Idle;

impl Backend for Idle {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
        Err(Fault::engine("the benchmark asks no question"))
    }

    fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
        Ok(())
    }
}

/// The fact `p(n).`
fn fact(n: i32) -> Statement {
    let name = Name::new("p").expect("a valid identifier");
    Statement::from(Rule::fact(Atom::new(name, [Term::from(Symbol::number(n))])))
}

/// One assertion into a knowledge base of each size.
fn assertion(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("rebuild");
    for size in SIZES {
        let knowledge = Program::of((0..size).map(fact));
        group.bench_with_input(
            BenchmarkId::new("assert", size),
            &knowledge,
            |bencher, knowledge| {
                bencher.iter_batched(
                    || Agent::new(knowledge.clone(), Idle),
                    |mut agent| {
                        let handle = agent.assert(fact(-1)).expect("an assertion is accepted");
                        (agent, handle)
                    },
                    BatchSize::LargeInput,
                );
            },
        );
    }
    group.finish();
}

fn main() {
    let mut criterion = Criterion::default().configure_from_args();
    assertion(&mut criterion);
    criterion.final_summary();
}
