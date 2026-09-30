//! The knowledge-base modification loop (docs/design/solve.md §6.2): asserting
//! and retracting statements against the owned knowledge base, with the
//! retraction realisation disclosed at assertion, a stale or foreign handle
//! refused rather than silently obeyed, and the engine left to the ask step.

use std::cell::RefCell;
use std::rc::Rc;

use themelios_program::program::Part;
use themelios_program::{Atom, Name, Program, Sign, Statement, Symbol};
use themelios_solve::agent::{Agent, RetractionClass};
use themelios_solve::bridge::{Door, GroundProgram};
use themelios_solve::contract::{
    Backend, Capabilities, Fault, GroundOptions, Locus, SolveRequest, TruthValue,
};
use themelios_solve::extend::Facts;
use themelios_solve::outcome::Solved;

// ---- a recording backend the laws inspect ----

/// One thing the backend was asked to do, in the order asked.
#[derive(Clone, PartialEq, Debug)]
enum Step {
    Reset,
    Lower,
    Ground(Vec<Part>),
    Assign(Symbol, TruthValue),
}

/// What the backend was asked to do, shared with the test that owns the agent.
#[derive(Default)]
struct Records {
    lowered: u32,
    grounded: u32,
    assigned: Vec<(Symbol, TruthValue)>,
    steps: Vec<Step>,
}

/// A backend that records what it is asked and never actually solves — the laws
/// drive the loop and read the record, they do not read models.
struct Recorder {
    externals: bool,
    multi_shot: bool,
    records: Rc<RefCell<Records>>,
}

impl Backend for Recorder {
    fn capabilities(&self) -> Capabilities {
        let mut capabilities = Capabilities::default();
        capabilities.externals = self.externals;
        capabilities.multi_shot = self.multi_shot;
        capabilities
    }

    fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
        Err(Fault::unsupported())
    }

    fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
        let mut records = self.records.borrow_mut();
        records.lowered += 1;
        records.steps.push(Step::Lower);
        Ok(())
    }

    fn ground_program(&self) -> Option<&GroundProgram> {
        None
    }

    fn ground(&mut self, parts: &[Part], _options: &GroundOptions) -> Result<(), Fault> {
        if parts
            .iter()
            .any(|part| part.key().name == identifier(REFUSED_PART))
        {
            return Err(Fault::request("the recorder grounds no such part"));
        }
        let mut records = self.records.borrow_mut();
        records.grounded += 1;
        records.steps.push(Step::Ground(parts.to_vec()));
        Ok(())
    }

    fn assign_external(&mut self, external: Symbol, value: TruthValue) -> Result<(), Fault> {
        let mut records = self.records.borrow_mut();
        records.assigned.push((external.clone(), value));
        records.steps.push(Step::Assign(external, value));
        Ok(())
    }

    fn reset(&mut self) -> Result<(), Fault> {
        self.records.borrow_mut().steps.push(Step::Reset);
        Ok(())
    }
}

/// The name of a part the recorder refuses to ground.
const REFUSED_PART: &str = "refused";

/// An agent over the knowledge `program` and a fresh recorder, with the record
/// handed back so a law can read what the backend was asked.
fn agent_over(
    program: Program,
    externals: bool,
    multi_shot: bool,
) -> (Agent<Recorder>, Rc<RefCell<Records>>) {
    let records = Rc::new(RefCell::new(Records::default()));
    let backend = Recorder {
        externals,
        multi_shot,
        records: Rc::clone(&records),
    };
    (Agent::new(program, backend), records)
}

fn identifier(name: &str) -> Name {
    Name::new(name).expect("a valid identifier")
}

/// A ground fact `name.`.
fn fact(name: &str) -> Statement {
    Statement::from(themelios_program::Rule::fact(Atom::constant(identifier(
        name,
    ))))
}

/// The ground atom symbol `p(n)`.
fn atom_symbol(n: i32) -> Symbol {
    Symbol::function(identifier("p"), [Symbol::number(n)], Sign::Positive)
}

/// A fact source denoting the atoms `p(n)` for each given `n`.
struct Predicates(Vec<i32>);

impl Facts for Predicates {
    fn facts(&self) -> impl Iterator<Item = Symbol> {
        self.0.iter().map(|&n| atom_symbol(n))
    }
}

/// A fact source denoting bare numbers — not atoms a program asserts.
struct Numbers(Vec<i32>);

impl Facts for Numbers {
    fn facts(&self) -> impl Iterator<Item = Symbol> {
        self.0.iter().map(|&n| Symbol::number(n))
    }
}

/// A fact source whose first symbol is an atom and whose second is not — a
/// refusal that must leave no trace of the first.
struct AtomThenNumber;

impl Facts for AtomThenNumber {
    fn facts(&self) -> impl Iterator<Item = Symbol> {
        [atom_symbol(1), Symbol::number(2)].into_iter()
    }
}

const SINGLE_SHOT: bool = false;
const MULTI_SHOT: bool = true;
const NO_EXTERNALS: bool = false;
const GUARDS_EXTERNALS: bool = true;

// ---- the owned knowledge base ----

#[test]
fn an_asserted_statement_grows_the_knowledge_base() {
    let (mut agent, _records) = agent_over(Program::of([fact("a")]), NO_EXTERNALS, SINGLE_SHOT);
    agent.assert(fact("b")).expect("assert succeeds");
    assert_eq!(agent.knowledge().statements().count(), 2);
}

#[test]
fn assert_then_retract_restores_the_knowledge_base() {
    let (mut agent, _records) = agent_over(Program::of([fact("a")]), NO_EXTERNALS, SINGLE_SHOT);
    let added = agent.assert(fact("b")).expect("assert succeeds");
    agent.retract(added).expect("retract succeeds");
    assert_eq!(agent.knowledge(), &Program::of([fact("a")]));
}

#[test]
fn a_multi_part_knowledge_base_round_trips_a_base_assertion() {
    use themelios_program::WithProvenance;
    use themelios_program::program::PartKey;
    let base_key = Program::empty().base().key().clone();
    let step_key = PartKey {
        name: identifier("step"),
        formals: Vec::new(),
    };
    let seed = Program::of_keyed_nodes([
        (base_key, WithProvenance::constructed(fact("a"))),
        (step_key, WithProvenance::constructed(fact("b"))),
    ]);
    let (mut agent, _records) = agent_over(seed.clone(), NO_EXTERNALS, SINGLE_SHOT);
    let added = agent.assert(fact("c")).expect("assert into base");
    agent.retract(added).expect("retract");
    assert_eq!(agent.knowledge(), &seed);
}

// ---- a spent or foreign handle refuses ----

#[test]
fn retract_of_a_stale_handle_refuses_at_the_request_locus() {
    let (mut agent, _records) = agent_over(Program::of([fact("a")]), NO_EXTERNALS, SINGLE_SHOT);
    let added = agent.assert(fact("b")).expect("assert succeeds");
    agent.retract(added).expect("the first retract succeeds");
    let again = agent.retract(added);
    assert_eq!(
        again.expect_err("the second retract refuses").locus(),
        Locus::Request
    );
}

#[test]
fn a_handle_from_another_agent_is_refused() {
    // `one` issues a handle at slot 0; `two` holds a different statement at the
    // same slot. Only the issuing ledger's brand keeps the handle from retracting
    // `two`'s statement.
    let (mut one, _records_one) = agent_over(Program::empty(), NO_EXTERNALS, SINGLE_SHOT);
    let (mut two, _records_two) =
        agent_over(Program::of([fact("floor")]), NO_EXTERNALS, SINGLE_SHOT);
    let issued_by_one = one.assert(fact("a")).expect("assert succeeds");
    let refused = two.retract(issued_by_one);
    assert_eq!(
        refused.expect_err("a foreign handle refuses").locus(),
        Locus::Request
    );
    assert_eq!(two.knowledge(), &Program::of([fact("floor")]));
}

#[test]
fn a_handle_whose_slot_was_reused_is_refused() {
    let (mut agent, _records) = agent_over(Program::empty(), NO_EXTERNALS, SINGLE_SHOT);
    let first = agent.assert(fact("a")).expect("assert a");
    agent.retract(first).expect("retract a frees its slot");
    let second = agent.assert(fact("b")).expect("assert b reuses the slot");
    // The reused-slot handle must not retract whatever now occupies the slot.
    assert_eq!(
        agent
            .retract(first)
            .expect_err("the reused-slot handle refuses")
            .locus(),
        Locus::Request
    );
    agent
        .retract(second)
        .expect("the live handle still retracts");
    assert_eq!(agent.knowledge(), &Program::empty());
}

#[test]
fn retracting_one_statement_leaves_the_other_handles_live() {
    let (mut agent, _records) = agent_over(Program::empty(), NO_EXTERNALS, SINGLE_SHOT);
    let first = agent.assert(fact("a")).expect("assert a");
    let second = agent.assert(fact("b")).expect("assert b");
    let third = agent.assert(fact("c")).expect("assert c");
    agent.retract(second).expect("retract the middle handle");
    agent
        .retract(first)
        .expect("the first handle is still live");
    agent
        .retract(third)
        .expect("the third handle is still live");
    assert_eq!(agent.knowledge(), &Program::empty());
}

// ---- the retraction class is disclosed ----

#[test]
fn a_plain_fact_discloses_the_rebuild_class() {
    let (mut agent, _records) = agent_over(Program::empty(), NO_EXTERNALS, SINGLE_SHOT);
    let added = agent.assert(fact("p")).expect("assert succeeds");
    assert_eq!(added.retraction_class(), RetractionClass::Rebuild);
}

#[test]
fn an_assertion_discloses_rebuild_though_the_backend_honours_externals() {
    // A toggle needs an externally guarded statement as well as the backend's
    // `externals`, and the agent guards none yet.
    let (mut agent, _records) = agent_over(Program::empty(), GUARDS_EXTERNALS, MULTI_SHOT);
    let added = agent.assert(fact("p")).expect("assert succeeds");
    assert_eq!(added.retraction_class(), RetractionClass::Rebuild);
}

#[test]
fn a_statement_over_an_externals_backend_still_retracts() {
    let (mut agent, _records) = agent_over(Program::empty(), GUARDS_EXTERNALS, MULTI_SHOT);
    let added = agent.assert(fact("p")).expect("assert succeeds");
    agent.retract(added).expect("the handle retracts");
    assert_eq!(agent.knowledge(), &Program::empty());
}

// ---- assert and retract leave the engine to the ask step ----

#[test]
fn neither_assert_nor_retract_lowers_the_backend() {
    let (mut agent, records) = agent_over(Program::empty(), GUARDS_EXTERNALS, MULTI_SHOT);
    let added = agent.assert(fact("a")).expect("assert succeeds");
    agent.retract(added).expect("retract succeeds");
    assert_eq!(records.borrow().lowered, 0);
}

// ---- the retained multi-shot seams reach the backend ----

#[test]
fn assigning_an_external_reaches_the_backend_seam() {
    let (mut agent, records) = agent_over(Program::empty(), GUARDS_EXTERNALS, MULTI_SHOT);
    let external = atom_symbol(1);
    agent
        .assign_external(external.clone(), TruthValue::False)
        .expect("assign succeeds");
    assert_eq!(
        records.borrow().assigned,
        vec![(external, TruthValue::False)]
    );
}

#[test]
fn grounding_reaches_the_backend_seam() {
    let (mut agent, records) = agent_over(Program::empty(), GUARDS_EXTERNALS, MULTI_SHOT);
    let no_parts: &[Part] = &[];
    agent.ground(no_parts).expect("ground succeeds");
    assert_eq!(records.borrow().grounded, 1);
}

// ---- a rebuild re-establishes what the loop retains ----

/// The part named `name`, with no formals and no statements.
fn part(name: &str) -> Part {
    Program::of_keyed_nodes([(
        themelios_program::program::PartKey {
            name: identifier(name),
            formals: Vec::new(),
        },
        themelios_program::WithProvenance::constructed(fact("x")),
    )])
    .parts()
    .find(|part| part.key().name == identifier(name))
    .cloned()
    .expect("the part was built")
}

/// Put a question to `agent`, and forget the recorder's refusal to solve: the
/// steps before the solve are what the laws read.
fn ask(agent: &mut Agent<Recorder>) {
    let _ = agent.solve();
}

#[test]
fn a_question_over_a_multi_shot_backend_resets_before_it_lowers() {
    let (mut agent, records) = agent_over(Program::empty(), NO_EXTERNALS, MULTI_SHOT);
    ask(&mut agent);
    ask(&mut agent);
    assert_eq!(
        records.borrow().steps,
        [Step::Reset, Step::Lower, Step::Reset, Step::Lower]
    );
}

#[test]
fn a_question_over_a_single_shot_backend_lowers_without_a_reset() {
    let (mut agent, records) = agent_over(Program::empty(), NO_EXTERNALS, SINGLE_SHOT);
    ask(&mut agent);
    ask(&mut agent);
    assert_eq!(records.borrow().steps, [Step::Lower, Step::Lower]);
}

#[test]
fn a_rebuild_grounds_the_retained_parts_before_it_assigns_the_externals() {
    let (mut agent, records) = agent_over(Program::empty(), GUARDS_EXTERNALS, MULTI_SHOT);
    let (first, second) = (atom_symbol(1), atom_symbol(2));
    agent.ground(&[part("step")]).expect("ground succeeds");
    agent
        .assign_external(second.clone(), TruthValue::True)
        .expect("assign succeeds");
    agent
        .assign_external(first.clone(), TruthValue::True)
        .expect("assign succeeds");
    records.borrow_mut().steps.clear();
    ask(&mut agent);
    assert_eq!(
        records.borrow().steps,
        [
            Step::Reset,
            Step::Lower,
            Step::Ground(vec![part("step")]),
            Step::Assign(first, TruthValue::True),
            Step::Assign(second, TruthValue::True),
        ]
    );
}

#[test]
fn a_rebuild_assigns_each_external_its_latest_value() {
    let (mut agent, records) = agent_over(Program::empty(), GUARDS_EXTERNALS, MULTI_SHOT);
    let external = atom_symbol(1);
    for value in [TruthValue::True, TruthValue::Free, TruthValue::False] {
        agent
            .assign_external(external.clone(), value)
            .expect("assign succeeds");
    }
    records.borrow_mut().steps.clear();
    ask(&mut agent);
    assert_eq!(
        records.borrow().steps,
        [
            Step::Reset,
            Step::Lower,
            Step::Assign(external, TruthValue::False)
        ]
    );
}

#[test]
fn a_refused_grounding_is_not_replayed() {
    let (mut agent, records) = agent_over(Program::empty(), GUARDS_EXTERNALS, MULTI_SHOT);
    agent
        .ground(&[part(REFUSED_PART)])
        .expect_err("the recorder refuses the part");
    records.borrow_mut().steps.clear();
    ask(&mut agent);
    assert_eq!(records.borrow().steps, [Step::Reset, Step::Lower]);
}

#[test]
fn a_grounding_brings_the_engine_level_before_it_grounds() {
    let (mut agent, records) = agent_over(Program::empty(), GUARDS_EXTERNALS, MULTI_SHOT);
    agent.ground(&[part("step")]).expect("ground succeeds");
    assert_eq!(
        records.borrow().steps,
        [Step::Reset, Step::Lower, Step::Ground(vec![part("step")])]
    );
}

// ---- observe and forget ----

#[test]
fn observe_records_each_fact_in_the_knowledge_base() {
    let (mut agent, _records) = agent_over(Program::empty(), NO_EXTERNALS, SINGLE_SHOT);
    let observation = agent.observe(Predicates(vec![1, 2, 3])).expect("observe");
    assert_eq!(observation.len(), 3);
    assert_eq!(agent.knowledge().statements().count(), 3);
}

#[test]
fn an_empty_observation_records_nothing() {
    let (mut agent, _records) = agent_over(Program::of([fact("a")]), NO_EXTERNALS, SINGLE_SHOT);
    let observation = agent
        .observe(Predicates(vec![]))
        .expect("observe of nothing");
    assert!(observation.is_empty());
    assert_eq!(agent.knowledge(), &Program::of([fact("a")]));
}

#[test]
fn forgetting_an_empty_observation_leaves_the_knowledge_base() {
    let (mut agent, _records) = agent_over(Program::of([fact("a")]), NO_EXTERNALS, SINGLE_SHOT);
    let observation = agent
        .observe(Predicates(vec![]))
        .expect("observe of nothing");
    agent.forget(observation).expect("forget of nothing");
    assert_eq!(agent.knowledge(), &Program::of([fact("a")]));
}

#[test]
fn forget_retracts_the_whole_observation() {
    let (mut agent, _records) = agent_over(Program::empty(), NO_EXTERNALS, SINGLE_SHOT);
    let observation = agent.observe(Predicates(vec![1, 2])).expect("observe");
    agent.forget(observation).expect("forget");
    assert_eq!(agent.knowledge(), &Program::empty());
}

#[test]
fn forget_of_a_spent_observation_refuses() {
    let (mut agent, _records) = agent_over(Program::empty(), NO_EXTERNALS, SINGLE_SHOT);
    let observation = agent.observe(Predicates(vec![1])).expect("observe");
    let spent = observation.clone();
    agent
        .forget(observation)
        .expect("the first forget succeeds");
    assert_eq!(
        agent
            .forget(spent)
            .expect_err("the second forget refuses")
            .locus(),
        Locus::Request
    );
}

#[test]
fn observe_refuses_a_fact_that_is_not_an_atom() {
    let (mut agent, _records) = agent_over(Program::empty(), NO_EXTERNALS, SINGLE_SHOT);
    assert_eq!(
        agent
            .observe(Numbers(vec![1]))
            .expect_err("a non-atom fact refuses")
            .locus(),
        Locus::Request
    );
}

#[test]
fn a_refused_observation_leaves_no_orphaned_fact() {
    let (mut agent, _records) = agent_over(Program::empty(), NO_EXTERNALS, SINGLE_SHOT);
    // The first symbol is a valid atom, the second is not: the observation must
    // refuse as a whole, recording neither.
    assert!(agent.observe(AtomThenNumber).is_err());
    assert_eq!(agent.knowledge(), &Program::empty());
    // A later assertion must not surface an orphan the refusal left in the
    // ledger — the rebuild would show it even if `knowledge()` looked empty.
    agent.assert(fact("z")).expect("assert after the refusal");
    assert_eq!(agent.knowledge(), &Program::of([fact("z")]));
}

// ---- content-equal assertions are independent handles ----

#[test]
fn duplicate_assertions_are_independent_handles() {
    let (mut agent, _records) = agent_over(Program::empty(), NO_EXTERNALS, SINGLE_SHOT);
    let first = agent.assert(fact("b")).expect("assert b once");
    let second = agent.assert(fact("b")).expect("assert b again");
    // The program is set-valued: b appears once though asserted twice.
    assert_eq!(agent.knowledge().statements().count(), 1);
    agent.retract(first).expect("retract one handle");
    // b remains — the second assertion still holds it.
    assert_eq!(agent.knowledge().statements().count(), 1);
    agent.retract(second).expect("retract the other handle");
    assert_eq!(agent.knowledge(), &Program::empty());
}

// ---- observe rebuilds once, not once per fact ----

#[test]
fn observe_scales_linearly_with_the_fact_count() {
    use std::hint::black_box;
    use std::time::Instant;

    const PROBE_FACTS: i32 = 1_500;
    const LINEAR_CEILING: u128 = 3;
    const SAMPLES: usize = 9;

    let observe_nanos = |count: i32| -> u128 {
        (0..SAMPLES)
            .map(|_| {
                let (mut agent, _records) = agent_over(Program::empty(), NO_EXTERNALS, SINGLE_SHOT);
                let source = Predicates((0..count).collect());
                let start = Instant::now();
                black_box(agent.observe(source).expect("observe succeeds"));
                start.elapsed().as_nanos()
            })
            .min()
            .expect("at least one sample")
    };

    let single = observe_nanos(PROBE_FACTS);
    let double = observe_nanos(PROBE_FACTS * 2);
    assert!(
        double < single.saturating_mul(LINEAR_CEILING),
        "observe grew worse than linearly — {single}ns for {PROBE_FACTS} facts, \
         {double}ns for {} — a rebuild per fact is the quadratic to avoid",
        PROBE_FACTS * 2,
    );
}

#[test]
fn forget_scales_linearly_with_the_fact_count() {
    use std::hint::black_box;
    use std::time::Instant;

    const PROBE_FACTS: i32 = 1_500;
    const LINEAR_CEILING: u128 = 3;
    const SAMPLES: usize = 9;

    let forget_nanos = |count: i32| -> u128 {
        (0..SAMPLES)
            .map(|_| {
                let (mut agent, _records) = agent_over(Program::empty(), NO_EXTERNALS, SINGLE_SHOT);
                let observation = agent
                    .observe(Predicates((0..count).collect()))
                    .expect("observe succeeds");
                let start = Instant::now();
                agent.forget(observation).expect("forget succeeds");
                black_box(agent.knowledge());
                start.elapsed().as_nanos()
            })
            .min()
            .expect("at least one sample")
    };

    let single = forget_nanos(PROBE_FACTS);
    let double = forget_nanos(PROBE_FACTS * 2);
    assert!(
        double < single.saturating_mul(LINEAR_CEILING),
        "forget grew worse than linearly — {single}ns for {PROBE_FACTS} facts, \
         {double}ns for {} — a rebuild per member is the quadratic to avoid",
        PROBE_FACTS * 2,
    );
}
