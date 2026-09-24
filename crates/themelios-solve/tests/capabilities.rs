//! Laws of the backend's capability declaration and the request-side values
//! (docs/design/solve.md §4.1, §4.2, §5.2, §6.3): a declaration is closed
//! bits and enums, read before a request is paid for; the empty declaration
//! declares nothing; the empty request is the pristine ask; and every value
//! is owned plain data.

use std::fmt::Debug;
use std::time::Duration;

use themelios_solve::contract::{
    BudgetSupport, Capabilities, ConsequenceRequest, ConsequenceSupport, GroundOptions, Mode,
    OptimizeRequest, SolveRequest, TheorySupport, TruthValue,
};

/// A time budget an ask might carry.
const BUDGET: Duration = Duration::from_secs(30);

/// A declaration beyond the empty one: an enumerating, multi-shot backend
/// that enforces a time budget.
fn some_declaration() -> Capabilities {
    let mut declared = Capabilities::default();
    declared.enumeration = true;
    declared.multi_shot = true;
    declared.budgets.time = true;
    declared
}

// --- the capability declaration ---

#[test]
fn the_empty_declaration_declares_nothing() {
    let empty = Capabilities::default();
    assert!(!empty.enumeration);
    assert!(!empty.optimization);
    assert!(!empty.externals);
    assert!(!empty.functions);
    assert!(!empty.propagators);
    assert!(!empty.multi_shot);
    assert!(!empty.assumptions);
    assert!(!empty.cancellation);
    assert_eq!(empty.theories, TheorySupport::default());
    assert_eq!(empty.budgets, BudgetSupport::default());
}

#[test]
fn the_empty_declaration_takes_the_derived_consequence_path() {
    // A backend that declares no native door is served by enumeration (§4.2).
    assert_eq!(
        Capabilities::default().native_consequences,
        ConsequenceSupport::DerivedByEnumeration
    );
}

#[test]
fn a_declaration_is_read_back_bit_for_bit() {
    let declared = some_declaration();
    assert!(declared.enumeration);
    assert!(declared.multi_shot);
    assert!(declared.budgets.time);
}

#[test]
fn a_declaration_differing_in_one_bit_is_unequal() {
    let mut declared = some_declaration();
    declared.cancellation = true;
    assert_ne!(declared, some_declaration());
}

#[test]
fn a_cloned_declaration_equals_its_original() {
    let declared = some_declaration();
    assert_eq!(declared.clone(), declared);
}

#[test]
fn a_declaration_s_debug_view_names_its_consequence_path() {
    let rendered = format!("{:?}", Capabilities::default());
    assert!(rendered.contains("DerivedByEnumeration"), "{rendered}");
}

// --- the consequence path, disclosed before it is paid for ---

#[test]
fn consequence_support_distinguishes_native_from_derived() {
    // The two paths are legible as distinct values, so a caller reads which
    // door a request takes before paying for it (§4.2).
    assert_ne!(
        ConsequenceSupport::Native,
        ConsequenceSupport::DerivedByEnumeration
    );
}

#[test]
fn the_absent_native_door_is_derivation() {
    assert_eq!(
        ConsequenceSupport::default(),
        ConsequenceSupport::DerivedByEnumeration
    );
}

#[test]
fn consequence_support_is_a_copyable_value() {
    let native = ConsequenceSupport::Native;
    let copy = native;
    assert_eq!(copy, native);
}

// --- the theory and budget declarations ---

#[test]
fn the_empty_theory_support_equals_itself() {
    assert_eq!(TheorySupport::default(), TheorySupport::default());
}

#[test]
fn a_cloned_theory_support_equals_its_original() {
    let theories = TheorySupport::default();
    assert_eq!(theories.clone(), theories);
}

#[test]
fn a_theory_support_s_debug_view_names_it() {
    assert_eq!(format!("{:?}", TheorySupport::default()), "TheorySupport");
}

#[test]
fn the_empty_budget_support_enforces_no_time_budget() {
    assert!(!BudgetSupport::default().time);
}

#[test]
fn a_budget_support_enforcing_time_differs_from_the_empty_one() {
    let mut budgets = BudgetSupport::default();
    budgets.time = true;
    assert_ne!(budgets, BudgetSupport::default());
}

#[test]
fn a_cloned_budget_support_equals_its_original() {
    let mut budgets = BudgetSupport::default();
    budgets.time = true;
    assert_eq!(budgets.clone(), budgets);
}

#[test]
fn a_budget_support_s_debug_view_names_its_time_bit() {
    let rendered = format!("{:?}", BudgetSupport::default());
    assert!(rendered.contains("time"), "{rendered}");
}

// --- the solve request ---

#[test]
fn the_empty_solve_request_is_the_pristine_ask() {
    let request = SolveRequest::default();
    assert_eq!(request, SolveRequest::default()); // deterministic, no ambient state
}

#[test]
fn the_pristine_ask_carries_no_time_budget() {
    assert_eq!(SolveRequest::default().time, None);
}

#[test]
fn a_solve_request_reads_back_its_time_budget() {
    let mut request = SolveRequest::default();
    request.time = Some(BUDGET);
    assert_eq!(request.time, Some(BUDGET));
}

#[test]
fn a_budgeted_solve_request_differs_from_the_pristine_ask() {
    let mut request = SolveRequest::default();
    request.time = Some(BUDGET);
    assert_ne!(request, SolveRequest::default());
}

#[test]
fn a_cloned_solve_request_equals_its_original() {
    let mut request = SolveRequest::default();
    request.time = Some(BUDGET);
    assert_eq!(request.clone(), request);
}

#[test]
fn a_solve_request_s_debug_view_names_its_budget() {
    let rendered = format!("{:?}", SolveRequest::default());
    assert!(rendered.contains("time"), "{rendered}");
}

// --- the optimize request ---

#[test]
fn the_empty_optimize_request_asks_for_no_trajectory() {
    assert!(!OptimizeRequest::default().report_trajectory);
}

#[test]
fn an_optimize_request_asking_for_the_trajectory_differs_from_the_empty_one() {
    let mut request = OptimizeRequest::default();
    request.report_trajectory = true;
    assert_ne!(request, OptimizeRequest::default());
}

#[test]
fn a_cloned_optimize_request_equals_its_original() {
    let mut request = OptimizeRequest::default();
    request.report_trajectory = true;
    assert_eq!(request.clone(), request);
}

#[test]
fn an_optimize_request_s_debug_view_names_its_trajectory_bit() {
    let rendered = format!("{:?}", OptimizeRequest::default());
    assert!(rendered.contains("report_trajectory"), "{rendered}");
}

// --- the ground options ---

#[test]
fn the_empty_ground_options_equal_themselves() {
    assert_eq!(GroundOptions::default(), GroundOptions::default());
}

#[test]
fn cloned_ground_options_equal_their_original() {
    let options = GroundOptions::default();
    assert_eq!(options.clone(), options);
}

#[test]
fn the_ground_options_debug_view_names_them() {
    assert_eq!(format!("{:?}", GroundOptions::default()), "GroundOptions");
}

// --- truth values and modes ---

#[test]
fn the_truth_values_are_three_and_distinct() {
    assert_ne!(TruthValue::True, TruthValue::False);
    assert_ne!(TruthValue::False, TruthValue::Free);
    assert_ne!(TruthValue::True, TruthValue::Free);
}

#[test]
fn a_truth_value_is_a_copyable_value() {
    let free = TruthValue::Free;
    let copy = free;
    assert_eq!(copy, free);
}

#[test]
fn a_truth_value_s_debug_view_names_it() {
    assert_eq!(format!("{:?}", TruthValue::Free), "Free");
}

#[test]
fn the_modes_are_two_and_distinct() {
    assert_ne!(Mode::Cautious, Mode::Brave);
}

#[test]
fn a_mode_is_a_copyable_value() {
    let cautious = Mode::Cautious;
    let copy = cautious;
    assert_eq!(copy, cautious);
}

#[test]
fn a_mode_s_debug_view_names_it() {
    assert_eq!(format!("{:?}", Mode::Brave), "Brave");
}

// --- every value is owned plain data ---

#[test]
fn every_request_side_value_is_owned_plain_data() {
    // A request crosses to a backend that may live on another thread.
    fn plain<T: Send + Sync + Clone + PartialEq + Debug + 'static>() {}
    plain::<Capabilities>();
    plain::<ConsequenceSupport>();
    plain::<TheorySupport>();
    plain::<BudgetSupport>();
    plain::<SolveRequest>();
    plain::<OptimizeRequest>();
    plain::<ConsequenceRequest>();
    plain::<GroundOptions>();
    plain::<TruthValue>();
    plain::<Mode>();
}

#[test]
fn every_closed_enum_is_copy() {
    fn copy<T: Copy>() {}
    copy::<ConsequenceSupport>();
    copy::<TruthValue>();
    copy::<Mode>();
}
