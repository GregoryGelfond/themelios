//! Laws of the extension surface (docs/design/solve.md §7–§9): a `Facts`
//! value denotes the set of ground atoms it stands for; an `@`-function is
//! called through the registration door as a trait object, yields its
//! results as symbols, and refuses a call outside its domain with a typed
//! ground fault that carries a locus — never a repaired value; a backend that
//! declares no `functions` capability refuses the registration as a typed
//! request fault; and a reader extracts a value from an answer set, the
//! inverse of `Facts`.

use std::error::Error;
use std::fmt::Debug;

use themelios_program::{AnswerSet, Name, Sign, Symbol};
use themelios_solve::bridge::{Door, GroundProgram};
use themelios_solve::contract::{Backend, Capabilities, Fault, Locus, SolveRequest};
use themelios_solve::extend::{Extract, ExtractError, Facts, Function, GroundFault};
use themelios_solve::outcome::Solved;

/// The edge relation's predicate name.
const EDGE: &str = "edge";
/// A path of two edges, the facts a graph value denotes.
const PATH: [(i32, i32); 2] = [(1, 2), (2, 3)];
/// What the successor function says of a call that is not one number.
const NOT_ONE_NUMBER: &str = "succ takes one number";
/// What the successor function says of the number whose successor the
/// `i32` width cannot represent.
const OUT_OF_RANGE: &str = "the successor of the greatest number is not representable";
/// A limit of the environment reached.
const RESOURCE_MESSAGE: &str = "the symbol table is full";
/// A number whose successor is representable.
const SOME_NUMBER: i32 = 2;

/// The ground atom `edge(from, to)`.
fn edge_symbol(from: i32, to: i32) -> Symbol {
    Symbol::function(
        Name::new(EDGE).expect("an identifier"),
        [Symbol::number(from), Symbol::number(to)],
        Sign::Positive,
    )
}

/// A graph's edges, a Rust value denoting the `edge/2` facts it holds (§7.3).
struct Edges(Vec<(i32, i32)>);

impl Facts for Edges {
    fn facts(&self) -> impl Iterator<Item = Symbol> {
        self.0.iter().map(|&(from, to)| edge_symbol(from, to))
    }
}

/// A backend that declares the empty capability set, so every registration
/// door is the refusing default it inherits (§4.1).
struct Undeclared;

impl Backend for Undeclared {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
        Err(Fault::unsupported())
    }

    fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
        Err(Fault::unsupported())
    }

    fn ground_program(&self) -> Option<&GroundProgram> {
        None
    }
}

/// An `@`-function that yields nothing, whatever its arguments.
struct NoopFunction;

impl Function for NoopFunction {
    fn call(&self, _arguments: &[Symbol]) -> Result<Vec<Symbol>, GroundFault> {
        Ok(Vec::new())
    }
}

/// The successor function, `@succ(n)`: one result, `n + 1`. It refuses a call
/// that is not one number, and the number whose successor the `i32` width
/// cannot represent (§7.2) — refused, never wrapped.
struct Successor;

impl Function for Successor {
    fn call(&self, arguments: &[Symbol]) -> Result<Vec<Symbol>, GroundFault> {
        match arguments {
            [Symbol::Number(n)] => n
                .checked_add(1)
                .map(|successor| vec![Symbol::number(successor)])
                .ok_or_else(|| GroundFault::refused(OUT_OF_RANGE)),
            _ => Err(GroundFault::refused(NOT_ONE_NUMBER)),
        }
    }
}

/// The number of atoms an answer set holds, read through `Extract` (§9).
#[derive(PartialEq, Debug)]
struct AtomCount(usize);

impl Extract for AtomCount {
    fn extract(answer_set: &AnswerSet) -> Result<Self, ExtractError> {
        Ok(AtomCount(answer_set.len()))
    }
}

// --- the bulk conversion (§7.3) ---

#[test]
fn a_facts_value_denotes_its_ground_atoms() {
    let atoms: Vec<Symbol> = Edges(PATH.to_vec()).facts().collect();
    assert_eq!(atoms, vec![edge_symbol(1, 2), edge_symbol(2, 3)]);
}

// --- the registration door (§4.1, §7.1) ---

#[test]
fn a_function_offered_without_the_capability_is_refused() {
    let mut backend = Undeclared;
    assert!(!backend.capabilities().functions);
    assert_eq!(
        backend.register_function(Box::new(NoopFunction)).err(),
        Some(Fault::unsupported())
    );
}

// --- the function, called through the door (§7.1, §7.2) ---

#[test]
fn a_function_is_called_as_a_trait_object() {
    // The door holds a function boxed (§4.1), so its calling surface is
    // reached through the object.
    let function: Box<dyn Function> = Box::new(Successor);
    assert_eq!(
        function.call(&[Symbol::number(SOME_NUMBER)]),
        Ok(vec![Symbol::number(SOME_NUMBER + 1)])
    );
}

#[test]
fn a_function_refuses_a_call_outside_its_domain() {
    assert_eq!(
        Successor.call(&[]),
        Err(GroundFault::refused(NOT_ONE_NUMBER))
    );
}

#[test]
fn a_function_refuses_a_result_the_symbol_width_cannot_carry() {
    // Refusal beats repair at the ASP boundary (§7.2): no wrap-around.
    assert_eq!(
        Successor.call(&[Symbol::number(i32::MAX)]),
        Err(GroundFault::refused(OUT_OF_RANGE))
    );
}

// --- the ground fault (§7.1) ---

#[test]
fn a_refused_call_blames_the_program() {
    assert_eq!(GroundFault::refused(NOT_ONE_NUMBER).locus(), Locus::Program);
}

#[test]
fn a_resource_ground_fault_reports_the_resource_locus() {
    assert_eq!(
        GroundFault::resource(RESOURCE_MESSAGE).locus(),
        Locus::Resource
    );
}

#[test]
fn a_ground_fault_displays_its_message() {
    assert_eq!(
        GroundFault::refused(NOT_ONE_NUMBER).to_string(),
        NOT_ONE_NUMBER
    );
}

#[test]
fn a_ground_fault_is_an_error_without_a_source() {
    let fault = GroundFault::resource(RESOURCE_MESSAGE);
    let error: &dyn Error = &fault;
    assert!(error.source().is_none());
}

#[test]
fn a_ground_fault_is_owned_plain_data() {
    fn plain<T: Send + Sync + Clone + PartialEq + Debug + 'static>() {}
    plain::<GroundFault>();
}

#[test]
fn ground_faults_differing_only_in_locus_are_unequal() {
    assert_ne!(
        GroundFault::refused(RESOURCE_MESSAGE),
        GroundFault::resource(RESOURCE_MESSAGE)
    );
}

// --- read-time extraction (§9) ---

#[test]
fn a_reader_extracts_a_value_from_an_answer_set() {
    // The inverse of `Facts`: what a value denotes, read back as a value.
    let model: AnswerSet = Edges(PATH.to_vec()).facts().collect();
    assert_eq!(AtomCount::extract(&model), Ok(AtomCount(PATH.len())));
}
