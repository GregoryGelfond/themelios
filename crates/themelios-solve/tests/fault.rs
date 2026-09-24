//! Laws of the solve tier's fault vocabulary (docs/design/solve.md §5.4): a
//! fault owns its model — a closed locus, a message that is never empty, a
//! source label only where it has one, and the backend-bug bit — renders
//! through `Display`, and lowers to a `base::Diagnostic` only where it is
//! located, never inventing a span for a fault that has none.

use std::collections::HashSet;
use std::error::Error;
use std::fmt::Debug;

use themelios_base::diagnostic::{Label, Severity, ToDiagnostic};
use themelios_base::source::SourceId;
use themelios_base::span::{ByteOffset, Location, Span};
use themelios_solve::contract::{Fault, Locus};

/// What an engine reported, verbatim.
const ENGINE_MESSAGE: &str = "clingo returned runtime error";
/// A request the agent cannot honour (docs/design/solve.md §6.2).
const REQUEST_MESSAGE: &str = "the statement was already retracted";
/// A limit of the environment reached.
const RESOURCE_MESSAGE: &str = "the symbol table is full";
/// A backend contract violation.
const ADAPTER_MESSAGE: &str = "the adapter reported a model outside a solve";
/// A statement the backend could not lower.
const PROGRAM_MESSAGE: &str = "malformed lowering";
/// What a label says about its region.
const LABEL_MESSAGE: &str = "this statement";

/// The one source the located faults point into.
const SOURCE: SourceId = SourceId::new(0);
/// A region with extent in that source.
const REGION_START: u32 = 8;
const REGION_END: u32 = 12;

/// The closed taxonomy, in declaration order.
const LOCI: [Locus; 5] = [
    Locus::Program,
    Locus::Request,
    Locus::Resource,
    Locus::Engine,
    Locus::Adapter,
];

fn some_location() -> Location {
    Location {
        source: SOURCE,
        span: Span::new(ByteOffset::new(REGION_START), ByteOffset::new(REGION_END))
            .expect("ordered endpoints"),
    }
}

fn some_label() -> Label {
    Label {
        location: some_location(),
        message: Some(LABEL_MESSAGE.to_owned()),
    }
}

/// One fault through every door, each raised with an empty message.
fn every_door_with_an_empty_message() -> [Fault; 5] {
    [
        Fault::engine(""),
        Fault::request(""),
        Fault::resource(""),
        Fault::adapter_bug(""),
        Fault::program("", some_label()),
    ]
}

// --- the engine door ---

#[test]
fn an_engine_fault_reports_the_engine_locus() {
    assert_eq!(Fault::engine(ENGINE_MESSAGE).locus(), Locus::Engine);
}

#[test]
fn an_engine_fault_is_not_a_backend_bug() {
    assert!(!Fault::engine(ENGINE_MESSAGE).is_backend_bug());
}

#[test]
fn an_unlocated_fault_does_not_lower_to_a_diagnostic() {
    // An engine fault has no source span, and none is invented for it.
    assert!(Fault::engine(ENGINE_MESSAGE).located().is_none());
}

#[test]
fn a_fault_displays_its_message() {
    assert_eq!(Fault::engine(ENGINE_MESSAGE).to_string(), ENGINE_MESSAGE);
}

// --- the request door ---

#[test]
fn a_request_fault_reports_the_request_locus() {
    assert_eq!(Fault::request(REQUEST_MESSAGE).locus(), Locus::Request);
}

#[test]
fn a_request_fault_is_not_a_backend_bug() {
    assert!(!Fault::request(REQUEST_MESSAGE).is_backend_bug());
}

#[test]
fn a_request_fault_is_unlocated() {
    assert!(Fault::request(REQUEST_MESSAGE).located().is_none());
}

#[test]
fn an_unsupported_request_is_a_request_locus() {
    assert_eq!(Fault::unsupported().locus(), Locus::Request);
}

#[test]
fn an_unsupported_request_is_not_a_backend_bug() {
    assert!(!Fault::unsupported().is_backend_bug());
}

#[test]
fn an_unsupported_request_says_so() {
    assert_eq!(Fault::unsupported().to_string(), "unsupported request");
}

#[test]
fn an_unsupported_request_is_unlocated() {
    assert!(Fault::unsupported().located().is_none());
}

// --- the resource door ---

#[test]
fn a_resource_fault_reports_the_resource_locus() {
    assert_eq!(Fault::resource(RESOURCE_MESSAGE).locus(), Locus::Resource);
}

#[test]
fn a_resource_fault_is_not_a_backend_bug() {
    assert!(!Fault::resource(RESOURCE_MESSAGE).is_backend_bug());
}

#[test]
fn a_resource_fault_is_unlocated() {
    assert!(Fault::resource(RESOURCE_MESSAGE).located().is_none());
}

// --- the adapter door ---

#[test]
fn an_adapter_bug_reports_the_adapter_locus() {
    assert_eq!(Fault::adapter_bug(ADAPTER_MESSAGE).locus(), Locus::Adapter);
}

#[test]
fn an_adapter_bug_is_a_backend_bug() {
    assert!(Fault::adapter_bug(ADAPTER_MESSAGE).is_backend_bug());
}

#[test]
fn an_adapter_bug_is_unlocated() {
    assert!(Fault::adapter_bug(ADAPTER_MESSAGE).located().is_none());
}

// --- the program door: the located fault and its lowering ---

#[test]
fn a_program_fault_reports_the_program_locus() {
    assert_eq!(
        Fault::program(PROGRAM_MESSAGE, some_label()).locus(),
        Locus::Program
    );
}

#[test]
fn a_program_fault_is_not_a_backend_bug() {
    assert!(!Fault::program(PROGRAM_MESSAGE, some_label()).is_backend_bug());
}

#[test]
fn a_program_fault_displays_its_message() {
    assert_eq!(
        Fault::program(PROGRAM_MESSAGE, some_label()).to_string(),
        PROGRAM_MESSAGE
    );
}

#[test]
fn a_located_program_fault_lowers_to_a_diagnostic_carrying_its_span() {
    let label = some_label();
    let fault = Fault::program(PROGRAM_MESSAGE, label.clone());
    let diagnostic = fault
        .located()
        .expect("a program fault is located")
        .to_diagnostic();
    assert_eq!(diagnostic.primary().location, label.location);
}

#[test]
fn a_program_fault_s_diagnostic_keeps_the_label_s_message() {
    let label = some_label();
    let fault = Fault::program(PROGRAM_MESSAGE, label.clone());
    let diagnostic = fault
        .located()
        .expect("a program fault is located")
        .to_diagnostic();
    assert_eq!(diagnostic.primary(), &label);
}

#[test]
fn a_program_fault_s_diagnostic_leads_with_the_fault_s_message() {
    let fault = Fault::program(PROGRAM_MESSAGE, some_label());
    let diagnostic = fault
        .located()
        .expect("a program fault is located")
        .to_diagnostic();
    assert_eq!(diagnostic.message(), PROGRAM_MESSAGE);
}

#[test]
fn a_program_fault_s_diagnostic_is_an_error() {
    let fault = Fault::program(PROGRAM_MESSAGE, some_label());
    let diagnostic = fault
        .located()
        .expect("a program fault is located")
        .to_diagnostic();
    assert_eq!(diagnostic.severity(), Severity::Error);
}

#[test]
fn a_program_fault_s_diagnostic_carries_the_solve_tier_s_identity() {
    let fault = Fault::program(PROGRAM_MESSAGE, some_label());
    let diagnostic = fault
        .located()
        .expect("a program fault is located")
        .to_diagnostic();
    assert_eq!(diagnostic.id().to_string(), "solve::program-fault");
}

#[test]
fn a_program_fault_s_diagnostic_carries_no_attachments() {
    // The lowering states the fault; it fabricates no narrative around it.
    let fault = Fault::program(PROGRAM_MESSAGE, some_label());
    let diagnostic = fault
        .located()
        .expect("a program fault is located")
        .to_diagnostic();
    assert!(diagnostic.secondary().is_empty());
    assert!(diagnostic.notes().is_empty());
    assert!(diagnostic.helps().is_empty());
}

#[test]
fn a_located_fault_is_a_copyable_view() {
    let fault = Fault::program(PROGRAM_MESSAGE, some_label());
    let located = fault.located().expect("a program fault is located");
    let copy = located;
    assert_eq!(copy, located);
    assert_eq!(copy.to_diagnostic(), located.to_diagnostic());
}

// --- the message is never empty ---

#[test]
fn a_fault_raised_without_a_message_still_says_something() {
    // The adapter builds faults from engine strings it does not control.
    assert!(!Fault::engine("").to_string().is_empty());
}

#[test]
fn every_door_replaces_an_empty_message() {
    for fault in every_door_with_an_empty_message() {
        assert!(
            !fault.to_string().is_empty(),
            "a fault at {:?} rendered empty",
            fault.locus()
        );
    }
}

#[test]
fn a_located_fault_raised_without_a_message_still_lowers() {
    let fault = Fault::program("", some_label());
    let diagnostic = fault
        .located()
        .expect("a program fault is located")
        .to_diagnostic();
    assert!(!diagnostic.message().is_empty());
}

// --- a fault is an error and plain data ---

#[test]
fn a_fault_is_an_error_without_a_source() {
    let fault = Fault::engine(ENGINE_MESSAGE);
    let error: &dyn Error = &fault;
    assert!(error.source().is_none());
}

#[test]
fn a_fault_is_owned_plain_data() {
    fn plain<T: Send + Sync + Clone + PartialEq + Debug + 'static>() {}
    plain::<Fault>();
    plain::<Locus>();
}

#[test]
fn a_cloned_fault_equals_its_original() {
    let fault = Fault::program(PROGRAM_MESSAGE, some_label());
    assert_eq!(fault.clone(), fault);
}

#[test]
fn faults_differing_only_in_locus_are_unequal() {
    assert_ne!(
        Fault::engine(ENGINE_MESSAGE),
        Fault::request(ENGINE_MESSAGE)
    );
}

#[test]
fn a_fault_s_debug_view_names_its_locus() {
    let rendered = format!("{:?}", Fault::adapter_bug(ADAPTER_MESSAGE));
    assert!(rendered.contains("Adapter"), "{rendered}");
}

#[test]
fn the_loci_are_five_and_distinct() {
    let distinct: HashSet<Locus> = LOCI.into_iter().collect();
    assert_eq!(distinct.len(), LOCI.len());
}
