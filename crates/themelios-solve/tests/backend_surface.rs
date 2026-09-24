//! Laws of the backend contract's surface (docs/design/solve.md §4.1, §4.2,
//! §4.3): the required surface is four methods; every capability-gated
//! method is provided with a default that refuses as a typed request fault,
//! so an undeclared capability is a refusal at the seam, never a compile
//! burden; the cancellation handle is absent unless a backend provides it;
//! and the contract is the one door, usable as a trait object.

use themelios_program::{Name, Symbol};
use themelios_solve::agent::Scenario;
use themelios_solve::bridge::{Door, GroundProgram};
use themelios_solve::contract::{
    Backend, Capabilities, Fault, GroundOptions, OptimizeRequest, SolveRequest, TruthValue,
};
use themelios_solve::extend::{Function, GroundFault, Propagator};
use themelios_solve::outcome::Solved;

/// A backend that implements the required surface alone (§4.1): it declares
/// nothing and serves nothing, so every provided method is the default it
/// inherits.
struct Nothing;

impl Backend for Nothing {
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

/// An `@`-function offered to a backend that evaluates none (§7).
struct NoFunction;

impl Function for NoFunction {
    fn call(&self, _arguments: &[Symbol]) -> Result<Vec<Symbol>, GroundFault> {
        Ok(Vec::new())
    }
}

/// A propagator offered to a backend that runs none (§8).
struct NoPropagator;

impl Propagator for NoPropagator {}

/// An external atom offered to a backend that honours none.
fn some_external() -> Symbol {
    Symbol::constant(Name::new("light").expect("an identifier"))
}

// --- the required surface ---

#[test]
fn a_backend_need_only_implement_the_required_surface() {
    // The gated methods carry defaults, so a type implementing the four
    // required methods alone is a backend: the minimality of §4.1, made a
    // test.
    fn is_a_backend<B: Backend>() {}
    is_a_backend::<Nothing>();
}

#[test]
fn a_backend_reads_back_the_declaration_it_makes() {
    assert_eq!(Nothing.capabilities(), Capabilities::default());
}

#[test]
fn the_contract_is_usable_as_a_trait_object() {
    // The core holds any engine behind the one door (§4.3), so the door is a
    // trait object.
    let boxed: Box<dyn Backend> = Box::new(Nothing);
    assert_eq!(boxed.capabilities(), Capabilities::default());
}

// --- the provided cancellation handle ---

#[test]
fn the_cancellation_handle_is_absent_by_default() {
    // `Some` exactly when `capabilities().cancellation`; a non-cancelling
    // backend inherits `None` (§4.1, §6.3).
    assert!(Nothing.interrupt().is_none());
}

// --- the capability-gated methods refuse by default ---

#[test]
fn optimize_refuses_without_optimization() {
    let mut nothing = Nothing;
    assert_eq!(
        nothing.optimize(&OptimizeRequest::default()).err(),
        Some(Fault::unsupported())
    );
}

#[test]
fn solve_assuming_refuses_without_assumptions() {
    let mut nothing = Nothing;
    assert_eq!(
        nothing
            .solve_assuming(&Scenario::default(), &SolveRequest::default())
            .err(),
        Some(Fault::unsupported())
    );
}

#[test]
fn ground_refuses_without_multi_shot() {
    let mut nothing = Nothing;
    assert_eq!(
        nothing.ground(&[], &GroundOptions::default()).err(),
        Some(Fault::unsupported())
    );
}

#[test]
fn assign_external_refuses_without_multi_shot() {
    let mut nothing = Nothing;
    assert_eq!(
        nothing
            .assign_external(some_external(), TruthValue::True)
            .err(),
        Some(Fault::unsupported())
    );
}

#[test]
fn reset_refuses_without_multi_shot() {
    let mut nothing = Nothing;
    assert_eq!(nothing.reset().err(), Some(Fault::unsupported()));
}

#[test]
fn register_function_refuses_without_functions() {
    let mut nothing = Nothing;
    assert_eq!(
        nothing.register_function(Box::new(NoFunction)).err(),
        Some(Fault::unsupported())
    );
}

#[test]
fn register_propagator_refuses_without_propagators() {
    let mut nothing = Nothing;
    assert_eq!(
        nothing.register_propagator(Box::new(NoPropagator)).err(),
        Some(Fault::unsupported())
    );
}
