//! Laws of the solve tier's fault vocabulary (docs/design/solve.md §5.4): a
//! fault owns its model — a closed locus, a message that is never empty, the
//! backend-bug bit, what it refused, and optionally the engine's typed cause —
//! renders through `Display`, and lowers to zero, one, or several base
//! diagnostics, never inventing a span for a fault that has none. What it
//! refused is a closed sum keyed by locus: a statement or a parse at the
//! program locus, the presupposition that failed at the request locus, and
//! nothing at the other three.

use std::collections::HashSet;
use std::error::Error;
use std::fmt::Debug;

use themelios_base::diagnostic::Severity;
use themelios_program::provenance::{Origin, Provenance, TransformTag, WithProvenance};
use themelios_program::raise::{RaisedSource, raise_source};
use themelios_program::{Atom, Dialect, Name, Rule, Source, SourceId, Statement};
use themelios_solve::bridge::NotAdmitted;
use themelios_solve::contract::{Capability, Fault, Locus, Presupposition, Refused};

/// What an engine reported, verbatim.
const ENGINE_MESSAGE: &str = "the engine died";
/// A request the agent cannot honour (docs/design/solve.md §6.2).
const REQUEST_MESSAGE: &str = "retract of a statement that is not live";
/// A limit of the environment reached.
const RESOURCE_MESSAGE: &str = "the symbol table is full";
/// A backend contract violation.
const ADAPTER_MESSAGE: &str = "a model held an atom and its contrary";
/// A statement the backend refuses.
const PROGRAM_MESSAGE: &str = "outside this backend's language";

/// The one statement the program faults refuse, as source text.
const RULE: &str = "a :- b.";
/// Text outside the language: two argument lists left open.
const NOT_IN_THE_LANGUAGE: &str = "p(. q(.";
/// Text in the language whose raise refuses: a definition repeated in its part.
const REPEATED_DEFINITION: &str = "#const n = 1. #const n = 1.";

/// The closed taxonomy, in declaration order.
const LOCI: [Locus; 5] = [
    Locus::Program,
    Locus::Request,
    Locus::Resource,
    Locus::Engine,
    Locus::Adapter,
];

/// The capabilities a refusal or a report names, in declaration order.
const CAPABILITIES: [Capability; 10] = [
    Capability::Optimization,
    Capability::NativeConsequences,
    Capability::Assumptions,
    Capability::MultiShot,
    Capability::Externals,
    Capability::Cancellation,
    Capability::TimeBudget,
    Capability::Functions,
    Capability::Propagators,
    Capability::GroundProgram,
];

/// `text` raised as the source `id` names, so its statements carry parsed
/// origins in that source. A fixture is far within the coordinate limit, so the
/// expect discharges an invariant.
fn raised_under(id: SourceId, text: &str) -> RaisedSource {
    let source = Source::new(id, text.to_owned()).expect("a fixture fits a source");
    raise_source(&source, Dialect::Clingo)
}

/// [`RULE`]'s one statement, raised as the source `id` names.
fn parsed_under(id: SourceId) -> WithProvenance<Statement> {
    raised_under(id, RULE)
        .program()
        .statements()
        .next()
        .expect("the rule raises to one statement")
        .clone()
}

/// A statement raised from text: it carries a parsed origin.
fn parsed_statement() -> WithProvenance<Statement> {
    parsed_under(SourceId::new(0))
}

/// The same statement raised from two sources, so its two copies are equal in
/// content and differ in origin.
fn the_same_statement_parsed_twice() -> (WithProvenance<Statement>, WithProvenance<Statement>) {
    (
        parsed_under(SourceId::new(0)),
        parsed_under(SourceId::new(1)),
    )
}

/// The least parsed origin a statement carries — the one its diagnostic's
/// primary label names (docs/design/program.md §6.3).
fn least_parsed_origin(statement: &WithProvenance<Statement>) -> Origin {
    statement
        .provenance()
        .origins()
        .find(|origin| matches!(origin, Origin::Parsed(_)))
        .cloned()
        .expect("a parsed statement carries a parsed origin")
}

/// A statement built through the program tier's constructors, written nowhere.
fn built_statement() -> Statement {
    Statement::from(Rule::fact(Atom::constant(
        Name::new("a").expect("a valid identifier"),
    )))
}

/// A parse outside the language, as the source `id` names, refused with its
/// syntax errors.
fn refused_syntax_under(id: SourceId) -> NotAdmitted {
    let raised = raised_under(id, NOT_IN_THE_LANGUAGE);
    assert!(
        raised.syntax_diagnostics().len() > 1,
        "the fixture has several errors"
    );
    NotAdmitted::Syntax(raised.syntax_diagnostics().into())
}

/// A parse outside the language, refused with its syntax errors.
fn refused_syntax() -> NotAdmitted {
    refused_syntax_under(SourceId::new(0))
}

/// A parse in the language whose raise refused, with the raise's whole batch.
fn refused_lowering() -> NotAdmitted {
    let raised = raised_under(SourceId::new(0), REPEATED_DEFINITION);
    assert!(
        raised.syntax_diagnostics().is_empty(),
        "the fixture is in the language"
    );
    assert!(
        !raised.lowering_diagnostics().is_empty(),
        "the fixture's raise refuses"
    );
    NotAdmitted::Lowering(raised.lowering_diagnostics().into())
}

/// One fault through every door, each raised with an empty message.
fn every_door_with_an_empty_message() -> [Fault; 6] {
    [
        Fault::engine(""),
        Fault::request("", Presupposition::NotLive),
        Fault::resource(""),
        Fault::adapter_bug(""),
        Fault::program("", &parsed_statement()),
        Fault::from(refused_syntax()),
    ]
}

// --- the engine, resource, and adapter doors refuse nothing ---

#[test]
fn an_engine_fault_reports_the_engine_locus() {
    assert_eq!(Fault::engine(ENGINE_MESSAGE).locus(), Locus::Engine);
}

#[test]
fn an_engine_fault_refuses_nothing() {
    assert!(matches!(
        Fault::engine(ENGINE_MESSAGE).refused(),
        Refused::Nothing
    ));
}

#[test]
fn an_engine_fault_lowers_to_no_diagnostic() {
    // An engine fault has no source span, and none is invented for it.
    assert!(Fault::engine(ENGINE_MESSAGE).diagnostics().is_empty());
}

#[test]
fn a_fault_displays_its_message() {
    assert_eq!(Fault::engine(ENGINE_MESSAGE).to_string(), ENGINE_MESSAGE);
}

#[test]
fn a_resource_fault_reports_the_resource_locus() {
    assert_eq!(Fault::resource(RESOURCE_MESSAGE).locus(), Locus::Resource);
}

#[test]
fn a_resource_fault_refuses_nothing() {
    assert!(matches!(
        Fault::resource(RESOURCE_MESSAGE).refused(),
        Refused::Nothing
    ));
}

#[test]
fn an_adapter_bug_reports_the_adapter_locus() {
    assert_eq!(Fault::adapter_bug(ADAPTER_MESSAGE).locus(), Locus::Adapter);
}

#[test]
fn an_adapter_bug_refuses_nothing() {
    assert!(matches!(
        Fault::adapter_bug(ADAPTER_MESSAGE).refused(),
        Refused::Nothing
    ));
}

#[test]
fn only_the_adapter_door_sets_the_bug_bit() {
    assert!(Fault::adapter_bug(ADAPTER_MESSAGE).is_backend_bug());
    for fault in [
        Fault::engine(ENGINE_MESSAGE),
        Fault::resource(RESOURCE_MESSAGE),
        Fault::request(REQUEST_MESSAGE, Presupposition::NotLive),
        Fault::unsupported(Capability::Optimization),
        Fault::program(PROGRAM_MESSAGE, &parsed_statement()),
        Fault::from(refused_syntax()),
    ] {
        assert!(!fault.is_backend_bug(), "{fault:?} carries the bug bit");
    }
}

// --- a request fault names the presupposition that failed ---

#[test]
fn a_request_fault_reports_the_request_locus() {
    assert_eq!(
        Fault::request(REQUEST_MESSAGE, Presupposition::NotLive).locus(),
        Locus::Request
    );
}

#[test]
fn a_request_fault_names_its_presupposition() {
    let fault = Fault::request(REQUEST_MESSAGE, Presupposition::NotLive);
    assert!(matches!(
        fault.refused(),
        Refused::Request(Presupposition::NotLive)
    ));
}

#[test]
fn a_request_fault_lowers_to_no_diagnostic() {
    let fault = Fault::request(REQUEST_MESSAGE, Presupposition::NotLive);
    assert!(fault.diagnostics().is_empty());
}

#[test]
fn an_unsupported_request_names_its_capability() {
    let fault = Fault::unsupported(Capability::Optimization);
    assert_eq!(fault.locus(), Locus::Request);
    assert!(matches!(
        fault.refused(),
        Refused::Request(Presupposition::Unsupported(Capability::Optimization))
    ));
}

#[test]
fn an_unsupported_request_s_message_names_its_capability() {
    for capability in CAPABILITIES {
        let message = Fault::unsupported(capability).to_string();
        assert!(
            message.contains(&capability.to_string()),
            "{message:?} does not name {capability}"
        );
    }
}

#[test]
fn the_capabilities_render_distinct_phrases() {
    let phrases: HashSet<String> = CAPABILITIES.iter().map(ToString::to_string).collect();
    assert_eq!(phrases.len(), CAPABILITIES.len());
}

// --- a program fault refers to its source ---

#[test]
fn a_program_fault_names_its_statement() {
    let statement = parsed_statement();
    let fault = Fault::program(PROGRAM_MESSAGE, &statement);
    assert_eq!(fault.locus(), Locus::Program);
    assert!(matches!(fault.refused(), Refused::Statement(refused) if refused == &statement));
}

#[test]
fn a_parsed_statement_s_fault_lowers_to_one_diagnostic_at_its_least_origin() {
    let statement = parsed_statement();
    let diagnostics = Fault::program(PROGRAM_MESSAGE, &statement).diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        Origin::Parsed(diagnostics[0].primary().location),
        least_parsed_origin(&statement)
    );
}

#[test]
fn a_program_fault_s_diagnostic_is_an_error() {
    let diagnostics = Fault::program(PROGRAM_MESSAGE, &parsed_statement()).diagnostics();
    assert_eq!(diagnostics[0].severity(), Severity::Error);
}

#[test]
fn a_program_fault_s_diagnostic_leads_with_the_fault_s_message() {
    let diagnostics = Fault::program(PROGRAM_MESSAGE, &parsed_statement()).diagnostics();
    assert_eq!(diagnostics[0].message(), PROGRAM_MESSAGE);
}

#[test]
fn a_program_fault_s_diagnostic_carries_the_solve_tier_s_identity() {
    let diagnostics = Fault::program(PROGRAM_MESSAGE, &parsed_statement()).diagnostics();
    assert_eq!(diagnostics[0].id().to_string(), "solve::program-fault");
}

#[test]
fn a_statement_parsed_once_lowers_with_no_attachments() {
    // The lowering states the fault; it fabricates no narrative around it.
    let diagnostics = Fault::program(PROGRAM_MESSAGE, &parsed_statement()).diagnostics();
    assert!(diagnostics[0].secondary().is_empty());
    assert!(diagnostics[0].notes().is_empty());
    assert!(diagnostics[0].helps().is_empty());
}

#[test]
fn a_statement_s_other_parsed_origins_are_its_secondary_labels() {
    // A content-equal collapse unions both copies' origins (program.md §6.3):
    // the least is the primary label, the other a secondary.
    let (here, there) = the_same_statement_parsed_twice();
    let merged = WithProvenance::new(
        here.get().clone(),
        here.provenance().clone().merge(there.provenance().clone()),
    );
    let diagnostics = Fault::program(PROGRAM_MESSAGE, &merged).diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        Origin::Parsed(diagnostics[0].primary().location),
        least_parsed_origin(&here)
    );
    let secondary: Vec<Origin> = diagnostics[0]
        .secondary()
        .iter()
        .map(|label| Origin::Parsed(label.location))
        .collect();
    assert_eq!(secondary, vec![least_parsed_origin(&there)]);
}

#[test]
fn a_statement_transformed_from_a_parsed_one_lowers_at_its_parsed_origin() {
    let statement = parsed_statement();
    let transformed = WithProvenance::new(
        statement.get().clone(),
        statement
            .provenance()
            .clone()
            .merge(Provenance::from(Origin::Transformed(TransformTag::new(
                "unpool",
            )))),
    );
    let diagnostics = Fault::program(PROGRAM_MESSAGE, &transformed).diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        Origin::Parsed(diagnostics[0].primary().location),
        least_parsed_origin(&statement)
    );
    assert!(diagnostics[0].secondary().is_empty());
}

#[test]
fn a_rust_built_statement_s_fault_lowers_to_no_diagnostic() {
    let statement = WithProvenance::constructed(built_statement());
    assert!(
        Fault::program(PROGRAM_MESSAGE, &statement)
            .diagnostics()
            .is_empty()
    );
}

#[test]
fn a_refused_parse_is_a_program_fault_carrying_its_refusal() {
    let refusal = refused_syntax();
    let fault = Fault::from(refusal.clone());
    assert_eq!(fault.locus(), Locus::Program);
    assert!(matches!(fault.refused(), Refused::Parse(carried) if carried == &refusal));
}

#[test]
fn a_refused_parse_lowers_to_one_diagnostic_per_syntax_error() {
    let refusal = refused_syntax();
    assert_eq!(
        Fault::from(refusal.clone()).diagnostics(),
        refusal.diagnostics()
    );
    let NotAdmitted::Syntax(errors) = &refusal else {
        unreachable!("the fixture is refused for its syntax")
    };
    assert_eq!(refusal.diagnostics().len(), errors.len());
}

#[test]
fn a_refused_raise_lowers_to_one_diagnostic_per_lowering_error() {
    let refusal = refused_lowering();
    assert_eq!(
        Fault::from(refusal.clone()).diagnostics(),
        refusal.diagnostics()
    );
    let NotAdmitted::Lowering(errors) = &refusal else {
        unreachable!("the fixture is refused at its raise")
    };
    assert_eq!(refusal.diagnostics().len(), errors.len());
}

#[test]
fn a_refused_parse_s_message_counts_its_errors() {
    assert_eq!(
        refused_syntax().to_string(),
        "the parse is not in the language (2 errors)"
    );
    assert_eq!(
        refused_lowering().to_string(),
        "the parse did not raise cleanly (1 error)"
    );
}

// --- equality: the message, the locus, the bit, and what was refused ---

#[test]
fn faults_refusing_content_equal_statements_at_different_origins_differ() {
    let (here, there) = the_same_statement_parsed_twice();
    assert_ne!(
        Fault::program(PROGRAM_MESSAGE, &here),
        Fault::program(PROGRAM_MESSAGE, &there)
    );
}

#[test]
fn faults_whose_statements_differ_only_in_annotations_are_equal() {
    let statement = parsed_statement();
    let annotated = WithProvenance::new(
        statement.get().clone(),
        statement.provenance().clone().with_doc("a note"),
    );
    assert_eq!(
        Fault::program(PROGRAM_MESSAGE, &statement),
        Fault::program(PROGRAM_MESSAGE, &annotated)
    );
}

#[test]
fn faults_differing_only_in_presupposition_are_unequal() {
    assert_ne!(
        Fault::request(REQUEST_MESSAGE, Presupposition::NotLive),
        Fault::request(REQUEST_MESSAGE, Presupposition::Spent)
    );
}

#[test]
fn faults_differing_only_in_locus_are_unequal() {
    assert_ne!(
        Fault::engine(ENGINE_MESSAGE),
        Fault::resource(ENGINE_MESSAGE)
    );
}

#[test]
fn faults_refusing_the_same_parse_are_equal() {
    assert_eq!(Fault::from(refused_syntax()), Fault::from(refused_syntax()));
}

#[test]
fn faults_refusing_parses_in_different_sources_differ() {
    // The same text, so the same message: only where its errors lie tells the
    // two refusals apart.
    let here = Fault::from(refused_syntax_under(SourceId::new(0)));
    let there = Fault::from(refused_syntax_under(SourceId::new(1)));
    assert_eq!(here.to_string(), there.to_string());
    assert_ne!(here, there);
}

#[test]
fn a_refused_statement_and_a_refused_parse_are_unequal() {
    // The same locus, message, and bit: only what was refused tells them apart.
    let refusal = refused_syntax();
    let statement = Fault::program(refusal.to_string(), &parsed_statement());
    assert_ne!(statement, Fault::from(refusal));
}

#[test]
fn a_cloned_fault_equals_its_original() {
    let fault = Fault::program(PROGRAM_MESSAGE, &parsed_statement());
    assert_eq!(fault.clone(), fault);
}

// --- the cause: the engine's own failure, detail and not identity ---

#[test]
fn a_cause_is_not_part_of_a_fault_s_equality() {
    let plain = Fault::engine(ENGINE_MESSAGE);
    let caused = Fault::engine(ENGINE_MESSAGE).caused_by(std::fmt::Error);
    assert_eq!(plain, caused);
}

#[test]
fn a_cause_is_the_fault_s_error_source() {
    let caused = Fault::engine(ENGINE_MESSAGE).caused_by(std::fmt::Error);
    let source = Error::source(&caused).expect("a cause");
    assert!(source.downcast_ref::<std::fmt::Error>().is_some());
}

#[test]
fn a_cloned_fault_keeps_its_cause() {
    let caused = Fault::engine(ENGINE_MESSAGE).caused_by(std::fmt::Error);
    let clone = caused.clone();
    let source = Error::source(&clone).expect("the clone's cause");
    assert!(source.is::<std::fmt::Error>());
}

#[test]
fn a_fault_without_a_cause_has_no_error_source() {
    assert!(Error::source(&Fault::engine(ENGINE_MESSAGE)).is_none());
}

// --- the message is never empty ---

#[test]
fn an_empty_message_is_replaced() {
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
fn a_program_fault_raised_without_a_message_still_lowers() {
    let diagnostics = Fault::program("", &parsed_statement()).diagnostics();
    assert!(!diagnostics[0].message().is_empty());
}

// --- a fault is plain data ---

#[test]
fn a_fault_is_plain_shareable_data() {
    fn is_plain<T: Send + Sync + Clone + Eq + Debug + 'static>() {}
    is_plain::<Fault>();
    is_plain::<Locus>();
    is_plain::<Presupposition>();
    is_plain::<NotAdmitted>();
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
