//! A program part a backend does not admit, refused as that part
//! (docs/design/solve.md §5.4, §6.3). A single-shot solve grounds the `base`
//! part alone, and a backend whose profile excludes a part beyond it refuses
//! the program at `lower` with a Program fault naming the part by its key:
//! through either door, a named part or one with formals, never naming a
//! statement in the part, and unlocated, since a part keeps no provenance. The
//! program lowered before stays, and an ordinary statement fault keeps its
//! location. Witnessed over a single-shot test backend whose profile admits the
//! `base` part alone; a real engine's profile is its own tests' to establish.

use themelios_base::source::{Source, SourceId};
use themelios_program::program::{Atom, PartKey, Program, Rule, Statement};
use themelios_program::provenance::{Origin, WithProvenance};
use themelios_program::symbol::{Name, Sign, Symbol};
use themelios_solve::agent::{Agent, Scenario};
use themelios_solve::bridge::{Admitted, Door};
use themelios_solve::contract::{Backend, Capabilities, Fault, Locus, Refused, SolveRequest};
use themelios_solve::outcome::{AnswerSet, Conclusion, Model, Run, ShowRule, Solved};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

/// The name `text`.
fn name(text: &str) -> Name {
    Name::new(text).expect("an identifier")
}

/// The part named `part`, with `formals`.
fn key(part: &str, formals: &[&str]) -> PartKey {
    PartKey {
        name: name(part),
        formals: formals.iter().map(|formal| name(formal)).collect(),
    }
}

/// The fact `atom.`, as a statement built in Rust.
fn fact(atom: &str) -> WithProvenance<Statement> {
    WithProvenance::constructed(Statement::from(Rule::fact(Atom::new(name(atom), []))))
}

/// The program built in Rust holding the fact `q.` in `base` and the fact `p.`
/// in `part`.
fn built_with(part: PartKey) -> Program {
    Program::of_keyed_nodes([(key("base", &[]), fact("q")), (part, fact("p"))])
}

/// The parse of `text`, admitted at Door A.
fn admitted(text: &str) -> Admitted {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("a short source");
    Admitted::of(&parse(&source, Dialect::Clingo)).expect("the text is admitted")
}

/// The answer set of the facts `atoms`.
fn answer_set(atoms: &[&str]) -> AnswerSet {
    atoms
        .iter()
        .map(|atom| Symbol::function(name(atom), [], Sign::Positive))
        .collect()
}

/// A run yielding one model of `set`, then closing the space.
struct One {
    set: Option<AnswerSet>,
}

impl Run for One {
    fn next_model(&mut self) -> Option<Result<Model, Fault>> {
        self.set.take().map(|set| Ok(Model::of(set)))
    }

    fn conclusion(&self) -> Option<Conclusion> {
        self.set.is_none().then_some(Conclusion::Exhausted)
    }
}

/// A single-shot backend whose profile admits the `base` part alone: its
/// `lower` refuses a program holding any other part, naming the first such part
/// by its key, and keeps the program lowered before; its `solve` answers the
/// facts `q` and `r` its program's base holds.
#[derive(Default)]
struct BaseOnly {
    lowered: Option<Program>,
}

impl Backend for BaseOnly {
    fn capabilities(&self) -> Capabilities {
        // Non-exhaustive, so declared by assignment.
        let mut capabilities = Capabilities::default();
        capabilities.enumeration = true;
        capabilities
    }

    fn lower(&mut self, door: Door<'_>) -> Result<(), Fault> {
        let program = door.program();
        if let Some(part) = program.parts().find(|part| *part.key() != key("base", &[])) {
            return Err(Fault::program_part(
                "this backend admits the base part alone",
                part.key(),
            ));
        }
        self.lowered = Some(program.clone());
        Ok(())
    }

    fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
        let Some(program) = &self.lowered else {
            return Err(Fault::engine("nothing is lowered"));
        };
        let held: Vec<&str> = ["q", "r"]
            .into_iter()
            .filter(|atom| {
                program
                    .base()
                    .statements()
                    .any(|statement| statement.get() == fact(atom).get())
            })
            .collect();
        Ok(Solved::running(
            Box::new(One {
                set: Some(answer_set(&held)),
            }),
            Scenario::default(),
            ShowRule::default(),
        ))
    }
}

/// The part a refused `lower` of `door` names, its fault at the program locus.
fn refused_part(door: Door<'_>) -> PartKey {
    let fault = BaseOnly::default()
        .lower(door)
        .expect_err("a part beyond the base is refused");
    assert_eq!(fault.locus(), Locus::Program, "{fault}");
    let Refused::Part(part) = fault.refused() else {
        panic!("the fault names a part: {fault}");
    };
    part.clone()
}

/// The answer sets a solve of `backend`'s lowered program yields.
fn answer_sets_of(backend: &mut BaseOnly) -> Vec<AnswerSet> {
    let mut solved = backend
        .solve(&SolveRequest::default())
        .expect("the backend solves");
    solved
        .all_models()
        .expect("a closed space")
        .into_iter()
        .map(|model| model.atoms().clone())
        .collect()
}

#[test]
fn a_named_part_built_in_rust_is_refused_as_that_part() {
    let program = built_with(key("foo", &[]));
    assert_eq!(refused_part(Door::Program(&program)), key("foo", &[]));
}

#[test]
fn a_part_with_formals_built_in_rust_is_refused_as_that_part() {
    let program = built_with(key("step", &["t"]));
    assert_eq!(refused_part(Door::Program(&program)), key("step", &["t"]));
}

#[test]
fn a_named_part_admitted_at_door_a_is_refused_as_that_part() {
    let admitted = admitted("q. #program foo. r.");
    assert_eq!(refused_part(Door::Parsed(&admitted)), key("foo", &[]));
}

#[test]
fn a_part_with_formals_admitted_at_door_a_is_refused_as_that_part() {
    let admitted = admitted("q. #program step(t). p(t).");
    assert_eq!(refused_part(Door::Parsed(&admitted)), key("step", &["t"]));
}

#[test]
fn a_part_fault_over_parsed_text_lowers_to_no_diagnostic() {
    // The text was parsed, yet the part keeps no provenance: no span is made up for it.
    let admitted = admitted("q. #program step(t). p(t).");
    let fault = BaseOnly::default()
        .lower(Door::Parsed(&admitted))
        .expect_err("a part beyond the base is refused");
    assert!(fault.diagnostics().is_empty(), "{:?}", fault.diagnostics());
}

#[test]
fn a_part_fault_renders_its_message() {
    let fault = Fault::program_part("the part is beyond this backend", &key("step", &["t"]));
    assert_eq!(fault.to_string(), "the part is beyond this backend");
}

#[test]
fn part_faults_naming_one_part_are_equal() {
    let part = key("step", &["t"]);
    assert_eq!(
        Fault::program_part("refused", &part),
        Fault::program_part("refused", &part)
    );
}

#[test]
fn part_faults_naming_different_parts_differ() {
    assert_ne!(
        Fault::program_part("refused", &key("step", &["t"])),
        Fault::program_part("refused", &key("step", &[]))
    );
}

#[test]
fn a_refused_part_leaves_the_program_lowered_before() {
    let mut backend = BaseOnly::default();
    backend
        .lower(Door::Program(&Program::of_nodes([fact("q")])))
        .expect("the base alone is admitted");
    let refused = admitted("r. #program step(t). p(t).");
    assert!(backend.lower(Door::Parsed(&refused)).is_err());
    assert_eq!(answer_sets_of(&mut backend), vec![answer_set(&["q"])]);
}

#[test]
fn an_agent_over_a_part_beyond_the_base_meets_the_part_fault() {
    let mut agent = Agent::new(built_with(key("step", &["t"])), BaseOnly::default());
    let fault = agent.solve().err().expect("the knowledge base is refused");
    assert!(
        matches!(fault.refused(), Refused::Part(part) if *part == key("step", &["t"])),
        "{fault}"
    );
}

#[test]
fn a_statement_fault_over_parsed_text_keeps_its_location() {
    // An ordinary Program fault still lowers to a diagnostic at the statement it refuses.
    let admitted = admitted("q.");
    let statement = admitted
        .statements()
        .next()
        .expect("the parse holds a statement");
    let refused = statement.statement();
    let diagnostics = Fault::program("this statement is refused", refused).diagnostics();
    let [diagnostic] = &diagnostics[..] else {
        panic!("one diagnostic: {diagnostics:?}");
    };
    let parsed = refused
        .provenance()
        .origins()
        .find(|origin| matches!(origin, Origin::Parsed(_)))
        .expect("a parsed statement carries a parsed origin");
    assert_eq!(&Origin::Parsed(diagnostic.primary().location), parsed);
}
